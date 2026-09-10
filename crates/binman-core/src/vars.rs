//! `{{variables}}`: finding them, resolving them, and which source wins.
//!
//! Five layers can define a variable. Lowest to highest:
//!
//! 1. collection vars — Postman `variable[]`, Bruno `collection.bru` and
//!    `folder.bru`
//! 2. the selected environment — `.env`, a Bruno environment, a Postman one
//! 3. vars the request declares for itself — Bruno `vars:pre-request`
//! 4. vars extracted from earlier responses
//! 5. overrides typed into the Vars tab
//!
//! The fourth is what makes chaining work: a token pulled out of a login
//! response beats the static environment for every request after it. The
//! fifth guarantees that whoever is at the keyboard can always win.

use std::collections::{BTreeMap, HashSet};
use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

pub type Vars = BTreeMap<String, String>;

static PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{([^}]+)\}\}").expect("the placeholder pattern compiles"));

static EMPTY: Vars = BTreeMap::new();

/// How many times a value that itself holds a placeholder is expanded: enough
/// for an environment that builds a URL out of a host variable, and a stop for
/// one that refers to itself.
const DEPTH: usize = 8;

/// Where each `{{name}}` sits in `text`, with the name trimmed as it is looked
/// up.
pub fn placeholders(text: &str) -> impl Iterator<Item = (Range<usize>, &str)> {
    PATTERN.captures_iter(text).filter_map(|captures| {
        let whole = captures.get(0)?;
        let name = captures.get(1)?.as_str().trim();
        Some((whole.range(), name))
    })
}

/// The names `text` refers to, each once, in the order they first appear.
pub fn scan(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    placeholders(text)
        .map(|(_, name)| name)
        .filter(|name| !name.is_empty() && seen.insert(*name))
        .map(str::to_string)
        .collect()
}

/// Replaces every placeholder `lookup` knows. Unknown ones are left exactly as
/// they were written, so a missing variable is visible in what gets sent.
pub fn resolve_with<'a>(text: &str, lookup: impl Fn(&str) -> Option<&'a str>) -> String {
    let mut current = text.to_string();
    for _ in 0..DEPTH {
        let next = PATTERN
            .replace_all(&current, |captures: &regex::Captures| {
                match lookup(captures[1].trim()) {
                    Some(value) => value.to_string(),
                    None => captures[0].to_string(),
                }
            })
            .into_owned();
        if next == current {
            break;
        }
        current = next;
    }
    current
}

pub fn resolve(text: &str, vars: &Vars) -> String {
    resolve_with(text, |name| vars.get(name).map(String::as_str))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Collection,
    Environment,
    Request,
    Extracted,
    Override,
}

impl Layer {
    pub fn label(self) -> &'static str {
        match self {
            Layer::Collection => "collection",
            Layer::Environment => "environment",
            Layer::Request => "request",
            Layer::Extracted => "extracted",
            Layer::Override => "override",
        }
    }
}

/// The five layers, borrowed from wherever each one is kept.
#[derive(Debug, Clone, Copy)]
pub struct Scope<'a> {
    pub collection: &'a Vars,
    pub environment: &'a Vars,
    pub request: &'a Vars,
    pub extracted: &'a Vars,
    pub overrides: &'a Vars,
}

impl Default for Scope<'_> {
    fn default() -> Self {
        Scope {
            collection: &EMPTY,
            environment: &EMPTY,
            request: &EMPTY,
            extracted: &EMPTY,
            overrides: &EMPTY,
        }
    }
}

impl<'a> Scope<'a> {
    /// The winning value for `name`, and the layer it came from.
    pub fn lookup(&self, name: &str) -> Option<(&'a str, Layer)> {
        [
            (self.overrides, Layer::Override),
            (self.extracted, Layer::Extracted),
            (self.request, Layer::Request),
            (self.environment, Layer::Environment),
            (self.collection, Layer::Collection),
        ]
        .into_iter()
        .find_map(|(vars, layer)| vars.get(name).map(|value| (value.as_str(), layer)))
    }

    pub fn resolve(&self, text: &str) -> String {
        resolve_with(text, |name| self.lookup(name).map(|(value, _)| value))
    }

    /// The names in `text` that no layer defines, even after expansion.
    pub fn unresolved(&self, text: &str) -> Vec<String> {
        scan(&self.resolve(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vars {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn resolves_what_it_knows() {
        let got = resolve(
            "Hello {{NAME}}, code={{ CODE }}!",
            &vars(&[("NAME", "world"), ("CODE", "42")]),
        );
        assert_eq!(got, "Hello world, code=42!");
    }

    #[test]
    fn an_unknown_placeholder_is_left_as_written() {
        assert_eq!(resolve("x={{MISSING}}", &Vars::new()), "x={{MISSING}}");
    }

    #[test]
    fn scanning_names_each_variable_once_in_order() {
        assert_eq!(
            scan("GET {{BASE}}/users/{{USER_ID}}?ref={{BASE}}"),
            vec!["BASE", "USER_ID"]
        );
        assert!(scan("no vars here").is_empty());
    }

    #[test]
    fn a_value_that_holds_a_placeholder_is_expanded_too() {
        let env = vars(&[("HOST", "api.example.com"), ("BASE", "https://{{HOST}}/v1")]);
        assert_eq!(
            resolve("{{BASE}}/users", &env),
            "https://api.example.com/v1/users"
        );
    }

    #[test]
    fn a_variable_that_refers_to_itself_stops() {
        let env = vars(&[("LOOP", "{{LOOP}}x")]);
        assert!(resolve("{{LOOP}}", &env).ends_with('x'));
    }

    #[test]
    fn layers_win_in_the_documented_order() {
        let collection = vars(&[
            ("A", "collection"),
            ("B", "collection"),
            ("C", "collection"),
        ]);
        let environment = vars(&[
            ("B", "environment"),
            ("C", "environment"),
            ("D", "environment"),
        ]);
        let request = vars(&[("C", "request"), ("D", "request")]);
        let extracted = vars(&[("D", "extracted"), ("E", "extracted")]);
        let overrides = vars(&[("E", "override")]);
        let scope = Scope {
            collection: &collection,
            environment: &environment,
            request: &request,
            extracted: &extracted,
            overrides: &overrides,
        };

        assert_eq!(scope.lookup("A"), Some(("collection", Layer::Collection)));
        assert_eq!(scope.lookup("B"), Some(("environment", Layer::Environment)));
        assert_eq!(scope.lookup("C"), Some(("request", Layer::Request)));
        assert_eq!(scope.lookup("D"), Some(("extracted", Layer::Extracted)));
        assert_eq!(scope.lookup("E"), Some(("override", Layer::Override)));
        assert_eq!(scope.unresolved("{{A}}{{Z}}"), vec!["Z"]);
    }
}
