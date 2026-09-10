//! Postman collection (v2.1) and environment exports.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::body::{self, BodyKind};
use crate::error::{Error, Result};
use crate::paths::lineage;
use crate::request::Request;
use crate::vars::Vars;

const COLLECTION_SUFFIX: &str = ".postman_collection.json";
const ENVIRONMENT_SUFFIX: &str = ".postman_environment.json";

pub fn is_collection(file_name: &str) -> bool {
    file_name.to_ascii_lowercase().ends_with(COLLECTION_SUFFIX)
}

#[derive(Debug, Default, Deserialize)]
pub struct Collection {
    #[serde(default)]
    pub info: Info,
    #[serde(default, rename = "item")]
    pub items: Vec<Item>,
    #[serde(default, rename = "variable")]
    variables: Vec<Pair>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Info {
    #[serde(default)]
    pub name: String,
}

/// A folder when it has items, a request when it has a request.
#[derive(Debug, Deserialize)]
pub struct Item {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    request: Option<RequestField>,
    #[serde(default, rename = "item")]
    pub items: Vec<Item>,
}

/// Postman accepts a bare URL where a request object would go.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RequestField {
    Url(String),
    Full(Box<PostmanRequest>),
}

#[derive(Debug, Default, Deserialize)]
struct PostmanRequest {
    #[serde(default)]
    method: String,
    #[serde(default)]
    url: PostmanUrl,
    #[serde(default)]
    header: Vec<Pair>,
    #[serde(default)]
    body: Option<PostmanBody>,
}

/// A URL is a string, or an object whose `raw` is the string.
#[derive(Debug, Default, Deserialize)]
#[serde(untagged)]
enum PostmanUrl {
    #[default]
    Missing,
    Raw(String),
    Object {
        #[serde(default)]
        raw: String,
    },
}

#[derive(Debug, Default, Deserialize)]
struct Pair {
    #[serde(default)]
    key: String,
    #[serde(default)]
    value: Value,
    #[serde(default)]
    disabled: bool,
}

#[derive(Debug, Default, Deserialize)]
struct PostmanBody {
    #[serde(default)]
    mode: String,
    #[serde(default)]
    raw: String,
    #[serde(default)]
    urlencoded: Vec<Pair>,
    #[serde(default)]
    formdata: Vec<FormPart>,
    #[serde(default)]
    graphql: Option<GraphqlBody>,
    #[serde(default)]
    options: Option<BodyOptions>,
}

#[derive(Debug, Default, Deserialize)]
struct FormPart {
    #[serde(default)]
    key: String,
    #[serde(default)]
    value: Value,
    #[serde(default)]
    src: Value,
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    disabled: bool,
}

#[derive(Debug, Default, Deserialize)]
struct GraphqlBody {
    #[serde(default)]
    query: String,
    #[serde(default)]
    variables: String,
}

#[derive(Debug, Default, Deserialize)]
struct BodyOptions {
    #[serde(default)]
    raw: Option<RawOptions>,
}

#[derive(Debug, Default, Deserialize)]
struct RawOptions {
    #[serde(default)]
    language: String,
}

/// A variable's value as text. Postman stores numbers and booleans as what
/// they are; a request only ever sees them as text.
fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

pub fn parse(bytes: &[u8]) -> Result<Collection> {
    serde_json::from_slice(bytes).map_err(|error| Error::parse("Postman collection", error))
}

impl Item {
    pub fn is_request(&self) -> bool {
        self.request.is_some()
    }

    pub fn method(&self) -> String {
        match &self.request {
            Some(RequestField::Full(request)) if !request.method.is_empty() => {
                request.method.to_ascii_uppercase()
            }
            _ => "GET".to_string(),
        }
    }
}

impl Collection {
    /// The collection's `variable[]`, the lowest layer of the variables.
    pub fn vars(&self) -> Vars {
        self.variables
            .iter()
            .filter(|pair| !pair.disabled && !pair.key.is_empty())
            .map(|pair| (pair.key.clone(), text(&pair.value)))
            .collect()
    }

    /// The item at `path`, one index per level of folders.
    pub fn item_at(&self, path: &[usize]) -> Option<&Item> {
        let (first, rest) = path.split_first()?;
        let mut item = self.items.get(*first)?;
        for index in rest {
            item = item.items.get(*index)?;
        }
        Some(item)
    }

    pub fn request_at(&self, path: &[usize]) -> Result<Request> {
        let item = self
            .item_at(path)
            .ok_or_else(|| Error::parse("Postman collection", "no item at that position"))?;
        match &item.request {
            None => Err(Error::parse(
                "Postman collection",
                format!("{} is a folder, not a request", item.name),
            )),
            Some(RequestField::Url(url)) => Ok(Request {
                method: "GET".into(),
                url: url.clone(),
                ..Request::default()
            }),
            Some(RequestField::Full(request)) => Ok(convert(request)),
        }
    }
}

fn convert(source: &PostmanRequest) -> Request {
    let mut request = Request {
        method: if source.method.is_empty() {
            "GET".into()
        } else {
            source.method.to_ascii_uppercase()
        },
        url: match &source.url {
            PostmanUrl::Missing => String::new(),
            PostmanUrl::Raw(raw) | PostmanUrl::Object { raw } => raw.clone(),
        },
        headers: source
            .header
            .iter()
            .filter(|pair| !pair.disabled && !pair.key.is_empty())
            .map(|pair| (pair.key.clone(), text(&pair.value)))
            .collect(),
        ..Request::default()
    };

    let Some(body) = &source.body else {
        return request;
    };
    match body.mode.as_str() {
        "raw" => {
            request.body = body.raw.clone();
            let language = body
                .options
                .as_ref()
                .and_then(|options| options.raw.as_ref())
                .map(|raw| raw.language.as_str())
                .unwrap_or("");
            request.kind = match language {
                "json" => Some(BodyKind::Json),
                "xml" => Some(BodyKind::Xml),
                "text" | "html" | "javascript" => Some(BodyKind::Text),
                _ => None,
            };
        }
        "urlencoded" => {
            let fields: Vec<(String, String)> = body
                .urlencoded
                .iter()
                .filter(|pair| !pair.disabled && !pair.key.is_empty())
                .map(|pair| (pair.key.clone(), text(&pair.value)))
                .collect();
            request.body = body::encode_form(&fields);
            request.kind = Some(BodyKind::Form);
        }
        "formdata" => {
            let fields: Vec<(String, String)> = body
                .formdata
                .iter()
                .filter(|part| !part.disabled && !part.key.is_empty())
                .map(|part| {
                    let value = if part.kind == "file" {
                        let source = match &part.src {
                            Value::Array(items) => items.first().map(text).unwrap_or_default(),
                            other => text(other),
                        };
                        format!("@{source}")
                    } else {
                        text(&part.value)
                    };
                    (part.key.clone(), value)
                })
                .collect();
            request.body = body::encode_form(&fields);
            request.kind = Some(BodyKind::Multipart);
        }
        "graphql" => {
            if let Some(graphql) = &body.graphql {
                let mut payload = serde_json::Map::new();
                payload.insert("query".into(), Value::String(graphql.query.clone()));
                if let Ok(variables) = serde_json::from_str::<Value>(&graphql.variables) {
                    payload.insert("variables".into(), variables);
                }
                request.body = Value::Object(payload).to_string();
                request.kind = Some(BodyKind::Json);
            }
        }
        _ => {}
    }
    request
}

#[derive(Debug, Deserialize)]
struct Environment {
    #[serde(default)]
    values: Vec<EnvironmentValue>,
}

#[derive(Debug, Deserialize)]
struct EnvironmentValue {
    #[serde(default)]
    key: String,
    #[serde(default)]
    value: Value,
    /// Postman leaves this out for enabled values, so absent means on.
    #[serde(default)]
    enabled: Option<bool>,
}

pub fn parse_environment(bytes: &[u8]) -> Result<Vars> {
    let environment: Environment = serde_json::from_slice(bytes)
        .map_err(|error| Error::parse("Postman environment", error))?;
    Ok(environment
        .values
        .iter()
        .filter(|value| value.enabled != Some(false) && !value.key.is_empty())
        .map(|value| (value.key.clone(), text(&value.value)))
        .collect())
}

pub fn load_environment(path: &Path) -> Result<Vars> {
    parse_environment(&std::fs::read(path)?)
}

/// `*.postman_environment.json` files from `dir` up to `root`. The deeper file
/// wins when two share a label, as with `.env` files.
pub fn find_environments(dir: &Path, root: &Path) -> Vec<(String, PathBuf)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for directory in lineage(dir, root) {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_ok_and(|kind| !kind.is_dir()))
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.to_ascii_lowercase().ends_with(ENVIRONMENT_SUFFIX))
            .collect();
        names.sort();
        for name in names {
            let label = name[..name.len() - ENVIRONMENT_SUFFIX.len()].to_string();
            if seen.insert(label.clone()) {
                out.push((label, directory.join(&name)));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    const COLLECTION: &str = r#"{
      "info": {"name": "demo"},
      "variable": [{"key": "BASE", "value": "https://api.example.com"}, {"key": "PAGE", "value": 2}],
      "item": [
        {
          "name": "Folder",
          "item": [
            {
              "name": "List users",
              "request": {
                "method": "GET",
                "url": "{{BASE}}/users",
                "header": [
                  {"key": "Accept", "value": "application/json"},
                  {"key": "X-Off", "value": "1", "disabled": true}
                ]
              }
            },
            {
              "name": "Create user",
              "request": {
                "method": "post",
                "url": {"raw": "{{BASE}}/users"},
                "body": {"mode": "raw", "raw": "{\"name\":\"Jane\"}", "options": {"raw": {"language": "json"}}}
              }
            }
          ]
        },
        {
          "name": "Login",
          "request": {
            "method": "POST",
            "url": "{{BASE}}/login",
            "body": {"mode": "urlencoded", "urlencoded": [{"key": "user", "value": "jane doe"}]}
          }
        }
      ]
    }"#;

    #[test]
    fn reads_requests_by_position() {
        let collection = parse(COLLECTION.as_bytes()).expect("parses");
        let vars = collection.vars();
        assert_eq!(vars["BASE"], "https://api.example.com");
        assert_eq!(vars["PAGE"], "2", "a number is read as text");

        let list = collection.request_at(&[0, 0]).expect("a request");
        assert_eq!(list.method, "GET");
        assert_eq!(list.url, "{{BASE}}/users");
        assert_eq!(
            list.headers,
            vec![("Accept".to_string(), "application/json".to_string())]
        );

        let create = collection.request_at(&[0, 1]).expect("a request");
        assert_eq!(create.method, "POST");
        assert_eq!(create.url, "{{BASE}}/users", "the object form of a URL");
        assert_eq!(create.kind, Some(BodyKind::Json));

        let login = collection.request_at(&[1]).expect("a request");
        assert_eq!(login.kind, Some(BodyKind::Form));
        assert_eq!(login.body, "user=jane+doe");
    }

    #[test]
    fn a_folder_is_not_a_request() {
        let collection = parse(COLLECTION.as_bytes()).unwrap();
        assert!(collection.request_at(&[0]).is_err());
        assert!(collection.request_at(&[9]).is_err());
    }

    #[test]
    fn environments_honour_enabled_and_default_it_to_on() {
        let vars = parse_environment(
            br#"{"name":"Dev","values":[
                {"key":"BASE","value":"https://dev","enabled":true},
                {"key":"TOKEN","value":"abc"},
                {"key":"OFF","value":"skip","enabled":false}]}"#,
        )
        .unwrap();
        assert_eq!(vars["BASE"], "https://dev");
        assert_eq!(vars["TOKEN"], "abc");
        assert!(!vars.contains_key("OFF"));
    }

    #[test]
    fn environments_are_found_walking_up() {
        let root = testing::scratch("postman-envs");
        let deep = root.join("api").join("v1");
        testing::write(
            &root.join("dev.postman_environment.json"),
            r#"{"values":[]}"#,
        );
        testing::write(
            &deep.join("prod.postman_environment.json"),
            r#"{"values":[]}"#,
        );

        let labels: Vec<String> = find_environments(&deep, &root)
            .into_iter()
            .map(|(label, _)| label)
            .collect();
        assert_eq!(labels, vec!["prod", "dev"]);
    }
}
