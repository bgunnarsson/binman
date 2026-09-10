//! The UI-agnostic half of binman.
//!
//! Everything about a request that is not how it looks on screen lives here:
//! reading the collection formats into one [`Request`], finding the
//! environments above it, resolving `{{variables}}` in the order v1 defined,
//! and sending it. The terminal front end works in these types and never
//! parses a file or opens a socket itself — the split binsql makes, so a
//! command mode can later be added over the same guarantees.

pub mod auth;
pub mod body;
pub mod client;
pub mod collection;
pub mod config;
pub mod dotenv;
pub mod env;
pub mod error;
pub mod extract;
pub mod formats;
pub mod history;
pub mod oauth2;
mod paths;
pub mod query;
pub mod request;
pub mod source;
#[cfg(test)]
mod testing;
pub mod vars;

pub use auth::AuthKind;
pub use body::BodyKind;
pub use client::{Client, Exchange, Prepared, SetCookie, Trace};
pub use config::Config;
pub use env::{EnvKind, EnvSource};
pub use error::{Error, Result};
pub use formats::Format;
pub use request::{METHODS, Request};
pub use source::{Loaded, Origin};
pub use vars::{Layer, Scope, Vars};
