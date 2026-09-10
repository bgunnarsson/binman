//! OpenAPI 3.x and Swagger 2.x specs, in YAML or JSON.
//!
//! A spec is recognised by what it says rather than by its name: any `.json`,
//! `.yaml` or `.yml` whose first kilobyte declares `openapi` or `swagger`.
//! Its operations are grouped by their first tag.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::error::{Error, Result};
use crate::request::Request;

#[derive(Debug, Default, Deserialize)]
pub struct Spec {
    #[serde(default)]
    servers: Vec<Server>,
    /// Swagger 2 names its server in three parts rather than one URL.
    #[serde(default)]
    host: Option<String>,
    #[serde(default, rename = "basePath")]
    base_path: Option<String>,
    #[serde(default)]
    schemes: Vec<String>,
    #[serde(default)]
    consumes: Vec<String>,
    #[serde(default)]
    paths: BTreeMap<String, PathItem>,
}

#[derive(Debug, Default, Deserialize)]
struct Server {
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PathItem {
    #[serde(default)]
    get: Option<Operation>,
    #[serde(default)]
    post: Option<Operation>,
    #[serde(default)]
    put: Option<Operation>,
    #[serde(default)]
    patch: Option<Operation>,
    #[serde(default)]
    delete: Option<Operation>,
    #[serde(default)]
    head: Option<Operation>,
    #[serde(default)]
    options: Option<Operation>,
    /// Shared by every method on the path.
    #[serde(default)]
    parameters: Vec<Parameter>,
}

#[derive(Debug, Default, Deserialize)]
struct Operation {
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    parameters: Vec<Parameter>,
    #[serde(default, rename = "requestBody")]
    request_body: Option<RequestBody>,
    #[serde(default)]
    consumes: Vec<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct Parameter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "in")]
    location: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RequestBody {
    #[serde(default)]
    content: BTreeMap<String, IgnoredAny>,
}

impl PathItem {
    fn operations(&self) -> [(&'static str, Option<&Operation>); 7] {
        [
            ("GET", self.get.as_ref()),
            ("POST", self.post.as_ref()),
            ("PUT", self.put.as_ref()),
            ("PATCH", self.patch.as_ref()),
            ("DELETE", self.delete.as_ref()),
            ("HEAD", self.head.as_ref()),
            ("OPTIONS", self.options.as_ref()),
        ]
    }

    fn operation(&self, method: &str) -> Option<&Operation> {
        self.operations()
            .into_iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(method))
            .and_then(|(_, operation)| operation)
    }
}

/// A set of operations sharing a tag, or "Default" for the untagged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub tag: String,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub route: String,
    pub method: String,
    pub summary: String,
}

/// Parses a spec; `file_name` decides JSON or YAML.
pub fn parse(bytes: &[u8], file_name: &str) -> Result<Spec> {
    if file_name.to_ascii_lowercase().ends_with(".json") {
        serde_json::from_slice(bytes).map_err(|error| Error::parse(file_name, error))
    } else {
        yaml_serde::from_slice(bytes).map_err(|error| Error::parse(file_name, error))
    }
}

/// Whether the start of a file declares it an OpenAPI or Swagger document.
pub fn is_spec(peek: &[u8], file_name: &str) -> bool {
    let peek = &peek[..peek.len().min(1024)];
    let text = String::from_utf8_lossy(peek);
    if file_name.to_ascii_lowercase().ends_with(".json") {
        return text.contains("\"openapi\"") || text.contains("\"swagger\"");
    }
    text.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("openapi:") || line.starts_with("swagger:")
    })
}

/// Every operation, grouped by first tag. Groups sort by tag; within one, by
/// path and then by method.
pub fn groups(spec: &Spec) -> Vec<Group> {
    let mut grouped: BTreeMap<String, Vec<Endpoint>> = BTreeMap::new();
    for (route, item) in &spec.paths {
        for (method, operation) in item.operations() {
            let Some(operation) = operation else {
                continue;
            };
            let tag = operation
                .tags
                .first()
                .cloned()
                .unwrap_or_else(|| "Default".to_string());
            grouped.entry(tag).or_default().push(Endpoint {
                route: route.clone(),
                method: method.to_string(),
                summary: operation.summary.clone().unwrap_or_default(),
            });
        }
    }
    grouped
        .into_iter()
        .map(|(tag, endpoints)| Group { tag, endpoints })
        .collect()
}

impl Spec {
    /// Where the API lives. The first server when it names a host; otherwise
    /// `{{URL}}`, so an environment can say where.
    fn base(&self) -> String {
        if let Some(url) = self
            .servers
            .first()
            .and_then(|server| server.url.as_deref())
        {
            let url = url.trim_end_matches('/');
            return if url.contains("://") {
                url.to_string()
            } else {
                format!("{{{{URL}}}}{url}")
            };
        }
        let base_path = self
            .base_path
            .as_deref()
            .unwrap_or("")
            .trim_end_matches('/');
        match &self.host {
            Some(host) => {
                let scheme = self.schemes.first().map(String::as_str).unwrap_or("https");
                format!("{scheme}://{host}{base_path}")
            }
            None => format!("{{{{URL}}}}{base_path}"),
        }
    }
}

/// The request an operation describes. Path parameters become
/// `{{placeholders}}`, query parameters an empty query string, header
/// parameters empty headers — filled in by hand, or by variables.
pub fn request(spec: &Spec, route: &str, method: &str) -> Request {
    let mut request = Request {
        method: method.to_ascii_uppercase(),
        url: format!("{}{}", spec.base(), template(route)),
        ..Request::default()
    };
    let Some(item) = spec.paths.get(route) else {
        return request;
    };
    let operation = item.operation(method);

    // An operation's own parameter replaces the path's of the same name and
    // location.
    let own = operation.map(|op| op.parameters.as_slice()).unwrap_or(&[]);
    let key = |parameter: &Parameter| (parameter.name.clone(), parameter.location.clone());
    let mut parameters: Vec<Parameter> = own.to_vec();
    parameters.extend(
        item.parameters
            .iter()
            .filter(|shared| !own.iter().any(|mine| key(mine) == key(shared)))
            .cloned(),
    );

    let mut query = Vec::new();
    let mut body_parameter = false;
    for parameter in &parameters {
        let Some(name) = parameter.name.as_deref() else {
            continue;
        };
        match parameter
            .location
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("query") => query.push(format!("{name}=")),
            Some("header") => request.headers.push((name.to_string(), String::new())),
            Some("body") => body_parameter = true,
            _ => {}
        }
    }
    if !query.is_empty() {
        request.url.push('?');
        request.url.push_str(&query.join("&"));
    }

    let is_json = |media: &str| media.starts_with("application/json");
    let json_body = operation.is_some_and(|operation| {
        let declared = operation
            .request_body
            .as_ref()
            .is_some_and(|body| body.content.keys().any(|media| is_json(media)));
        let consumes = if operation.consumes.is_empty() {
            &spec.consumes
        } else {
            &operation.consumes
        };
        declared || (body_parameter && (consumes.is_empty() || consumes.iter().any(|m| is_json(m))))
    });
    if json_body {
        request
            .headers
            .push(("Content-Type".into(), "application/json".into()));
    }
    request
}

/// `/users/{id}` → `/users/{{id}}`.
fn template(route: &str) -> String {
    let mut out = String::with_capacity(route.len() + 8);
    let mut rest = route;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str("{{");
        out.push_str(&rest[open + 1..open + close]);
        out.push_str("}}");
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{
      "openapi": "3.0.0",
      "info": {"title": "demo"},
      "servers": [{"url": "https://api.example.com/v1/"}],
      "paths": {
        "/users/{id}": {
          "parameters": [{"name": "verbose", "in": "query"}],
          "get": {
            "summary": "Get user",
            "tags": ["users"],
            "parameters": [{"name": "X-Trace", "in": "header"}]
          },
          "put": {
            "tags": ["users"],
            "requestBody": {"content": {"application/json": {"schema": {}}}}
          }
        },
        "/health": {"get": {"summary": "Health"}}
      }
    }"#;

    const YAML: &str = "openapi: 3.0.3
info:
  title: pets
paths:
  /pets:
    get:
      tags: [pets]
      summary:
      responses:
        200:
          description: ok
";

    #[test]
    fn builds_the_request_an_operation_describes() {
        let spec = parse(JSON.as_bytes(), "demo.json").expect("parses");
        let request = request(&spec, "/users/{id}", "get");
        assert_eq!(request.method, "GET");
        assert_eq!(
            request.url,
            "https://api.example.com/v1/users/{{id}}?verbose="
        );
        assert_eq!(request.header("X-Trace"), Some(""));

        let update = super::request(&spec, "/users/{id}", "PUT");
        assert_eq!(update.header("Content-Type"), Some("application/json"));
    }

    #[test]
    fn groups_by_first_tag() {
        let spec = parse(JSON.as_bytes(), "demo.json").unwrap();
        let groups = groups(&spec);
        let tags: Vec<&str> = groups.iter().map(|group| group.tag.as_str()).collect();
        assert_eq!(tags, vec!["Default", "users"]);
        assert_eq!(groups[1].endpoints.len(), 2);
        assert_eq!(groups[1].endpoints[0].summary, "Get user");
    }

    #[test]
    fn reads_yaml_and_leaves_the_host_to_a_variable() {
        let spec = parse(YAML.as_bytes(), "pets.yaml").expect("yaml parses");
        let request = request(&spec, "/pets", "GET");
        assert_eq!(request.url, "{{URL}}/pets");
        assert_eq!(groups(&spec)[0].endpoints[0].summary, "");
    }

    #[test]
    fn swagger_two_builds_its_base_from_host_and_path() {
        let spec = parse(
            br#"{"swagger":"2.0","host":"petstore.io","basePath":"/v2","schemes":["http"],
                 "paths":{"/pet":{"post":{"parameters":[{"name":"body","in":"body"}]}}}}"#,
            "petstore.json",
        )
        .unwrap();
        let request = request(&spec, "/pet", "POST");
        assert_eq!(request.url, "http://petstore.io/v2/pet");
        assert_eq!(request.header("Content-Type"), Some("application/json"));
    }

    #[test]
    fn recognises_a_spec_by_content() {
        assert!(is_spec(br#"{"openapi": "3.0.0"}"#, "x.json"));
        assert!(is_spec(b"# comment\nopenapi: 3.0.0\ninfo:", "x.yaml"));
        assert!(!is_spec(br#"{"foo": 1}"#, "x.json"));
        assert!(!is_spec(b"name: not a spec", "x.yml"));
    }
}
