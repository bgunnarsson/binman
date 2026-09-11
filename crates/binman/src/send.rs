//! `binman send`: one request file, sent from a script the way the terminal
//! front end would send it — the same environments, the same variables, the
//! same history.

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail};
use binman_core::history::History;
use binman_core::{Client, Format, Origin, Vars, Workspace, env};
use tokio_util::sync::CancellationToken;

use crate::app::tab::Tab;
use crate::app::{exchange, prepare, record};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub file: PathBuf,
    /// The environment, by name. Left out, it is the first one found, as it
    /// is when a request is opened in the terminal.
    pub env: Option<String>,
    /// Values given on the command line. They outrank every other layer, as
    /// values typed into the Vars section do.
    pub vars: Vars,
    /// The status line and headers on stdout ahead of the body, as `curl -i`.
    pub include: bool,
}

impl Options {
    /// Reads what follows `binman send`.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Options> {
        let mut args = args.into_iter();
        let mut file = None;
        let mut options = Options::default();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-i" | "--include" => options.include = true,
                "--env" => {
                    options.env = Some(args.next().context("--env needs an environment's name")?);
                }
                "--var" => {
                    let pair = args.next().context("--var needs NAME=VALUE")?;
                    let (name, value) = pair
                        .split_once('=')
                        .with_context(|| format!("--var {pair} is not NAME=VALUE"))?;
                    options.vars.insert(name.to_string(), value.to_string());
                }
                other if other.starts_with('-') => {
                    bail!("Unknown option {other} for binman send. Try --help.")
                }
                other => {
                    if file.replace(PathBuf::from(other)).is_some() {
                        bail!("binman send takes one request file");
                    }
                }
            }
        }
        options.file = file.context("binman send needs a request file: binman send <file>")?;
        Ok(options)
    }
}

/// Sends the request and writes the response: the body to `out`, and the
/// status line to `err` — or both, headers too, to `out` with `--include`.
/// Refuses, sending nothing, while any variable the request names is unset.
pub async fn run(
    options: &Options,
    collections: &Workspace,
    client: &Client,
    history: &History,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    let path = options
        .file
        .canonicalize()
        .with_context(|| options.file.display().to_string())?;
    if Format::of(&path).is_none() {
        bail!(
            "{} is not a request file — binman send reads .http, .bru and .graphql",
            options.file.display()
        );
    }
    let origin = Origin::File(path);
    // Environments are looked for up to the file's collection. A file from
    // outside them all has only its own directory to look in.
    let root = collections.root_for(origin.path());

    let loaded = origin.load(&root)?;
    let envs = env::discover(origin.dir(), &root);
    let env = match &options.env {
        None => (!envs.is_empty()).then_some(0),
        Some(_) if envs.is_empty() => {
            bail!("There are no environments above {}", options.file.display())
        }
        Some(name) => {
            let index = envs.iter().position(|source| &source.label == name);
            let known: Vec<&str> = envs.iter().map(|source| source.label.as_str()).collect();
            Some(index.ok_or_else(|| {
                anyhow!(
                    "There is no environment called {name} above {} — there is {}",
                    options.file.display(),
                    known.join(", ")
                )
            })?)
        }
    };

    let mut tab = Tab::blank(0, Vec::new(), None);
    tab.load(loaded, Some(origin), envs, env);
    tab.reload_env().map_err(anyhow::Error::msg)?;
    tab.overrides = options.vars.clone();

    // A terminal shows an unset variable and lets it go; a script has no one
    // looking, so here every one of them stops the send.
    let extracted = Vars::new();
    let scope = tab.scope(&extracted);
    let unset: Vec<String> = tab
        .referenced()
        .into_iter()
        .filter(|name| scope.lookup(name).is_none())
        .collect();
    if !unset.is_empty() {
        let verb = if unset.len() == 1 { "is" } else { "are" };
        bail!(
            "{} {verb} not set — --env picks an environment, and --var NAME=VALUE gives a value",
            unset.join(", ")
        );
    }

    let (prepared, grant) = prepare(&tab, &extracted, &root).map_err(anyhow::Error::msg)?;
    let started = Instant::now();
    let attempted = prepared.clone();
    let result = exchange(
        client,
        prepared,
        grant,
        &CancellationToken::new(),
        &[],
        |_| {},
    )
    .await;
    match &result {
        Ok(received) => record(
            history,
            &received.sent,
            received.exchange.status,
            received.exchange.trace.total,
        ),
        Err(_) => record(history, &attempted, 0, started.elapsed()),
    }

    let exchange = &result?.exchange;
    let status = format!(
        "{} {} {}",
        exchange.version, exchange.status, exchange.reason
    );
    if options.include {
        writeln!(out, "{}", status.trim_end())?;
        for (name, value) in &exchange.headers {
            writeln!(out, "{name}: {value}")?;
        }
        writeln!(out)?;
    } else {
        writeln!(err, "{}", status.trim_end())?;
    }
    out.write_all(&exchange.body)?;
    out.flush()?;
    Ok(())
}
