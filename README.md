# binman

An HTTP client for the terminal. It shows a directory of request files —
`.http`, Bruno `.bru`, `.graphql`, Postman collections, OpenAPI specs — as a
tree, opens requests in tabs, and shows the response beside them.

## Install

Each release carries builds for macOS, Linux and Windows on
[the releases page](https://github.com/bgunnarsson/binman/releases), with a
`checksums.txt`:

```sh
tar -xzf binman-2.0.0-darwin-arm64.tar.gz
sudo mv binman-2.0.0-darwin-arm64/binman /usr/local/bin/
```

Or build it with Rust 1.90 or newer:

```sh
cargo build --release   # target/release/binman
```

The tree's icons need a Nerd Font; without one they render as boxes.

## Configure

binman reads `~/.config/binman/config`, or `$XDG_CONFIG_HOME/binman/config`
when that is set:

```
# The directory holding your request files (required)
HTTP_FILES  = /path/to/your/collections

# 30s when left out; 0 for no limit, which a streaming endpoint needs
TIMEOUT     = 30s

# A client certificate for mTLS; both must be set
CLIENT_CERT = /path/to/client.crt
CLIENT_KEY  = /path/to/client.key
```

`HTTP_FILES` must be a directory that exists; a relative path is taken from
where binman starts. `TIMEOUT` takes durations like `30s`, `2m`, `1m30s` or
`500ms`. Values are taken literally — quotes stay in and `~` is not expanded —
and unknown keys are ignored. A `TIMEOUT` that does not parse, or a certificate
that cannot be read, stops binman at start with an error.

## Collection formats

The tree shows everything under `HTTP_FILES` that binman can open, directories
first. Hidden files are skipped, and a file that does not parse shows its error
in the tree. Only `.http` and `.bru` requests can be saved; the others can be
edited and sent.

A tab with no file — a new tab, or a request sent again from the history — asks
where to save it. The path has to be under `HTTP_FILES`, and the file is written
as `.http` unless the name ends in `.bru`. Nothing is written over a file that
is already there, and from then on `⌃S` saves to the new file.

### `.http`

```http
POST https://api.example.com/users
Content-Type: application/json
Authorization: Bearer {{TOKEN}}

{ "name": "Jane" }
```

The request line, headers, a blank line, then the body. Files written for VS
Code or JetBrains work too: `#` and `//` comments, a trailing `HTTP/1.1`, a bare
`https://…` URL as a GET, and `###` ending the body. Only the first request in
a file is read, and saving rewrites the whole file from it. A body with no
`Content-Type` is sent as JSON.

### `.bru`

JSON, XML, text, SPARQL, form, multipart and GraphQL bodies, with `@file(…)`
fields uploaded. `collection.bru` and `folder.bru` supply shared variables and
are hidden from the tree, as is the `environments/` directory beside a
`collection.bru`. Headers disabled with `~` are not sent, and scripts, tests
and asserts are not run. Saving rewrites only the blocks that changed.

### `.graphql`

Posted as the JSON document GraphQL servers expect:

```graphql
# URL: https://api.example.com/graphql
# Header: Authorization: Bearer {{TOKEN}}
---
query UserById($id: ID!) {
  user(id: $id) { name email }
}
---
{ "id": "42" }
```

The header block is optional, but the variables block needs it.

### Postman and OpenAPI

A `*.postman_collection.json` opens as its folders and requests, and its
`variable` list becomes the lowest layer of variables.

A `.json`, `.yaml` or `.yml` that declares `openapi` or `swagger` opens as its
operations, grouped by tag. Opening one fills in the method and URL, with path
parameters as `{{placeholders}}` and query and header parameters left empty;
`$ref`s are not followed. The base URL comes from `servers`, from Swagger 2's
`host` and `basePath`, or from `{{URL}}` when the spec names neither.

## Environments and variables

`{{NAME}}` works in the URL, headers, the body and auth fields. Environments are
found by walking up from the request's directory to the collection root, and
the nearest file of a name replaces any above it:

```
collections/
  .env              ← "default"
  .env.staging      ← "staging"
  cms/
    .env            ← the "default" for requests in here
```

Postman `*.postman_environment.json` files are found the same way, and Bruno
`environments/*.bru` files from the nearest `environments/` directory. Pick one
with `⌃E` or the picker in the header, where `e` edits its file. The choice
carries to requests opened later, and the file is read again before every send.
Values may be quoted and may use other variables: `BASE = https://{{HOST}}/api`.

Which value wins, lowest first:

1. Collection vars — Postman `variable[]`, Bruno `collection.bru` and `folder.bru`
2. The selected environment
3. The request's own — a `.bru` file's `vars` and `vars:pre-request`
4. Values extracted from responses
5. Values typed into the request's **Vars** section

**Vars** lists each variable in the request, its value and where it came from.
Only what you type there overrides, and `d` removes an override. A URL with an
unset variable is highlighted and not sent.

### Extracting values

Rules in a request's **Scripts** section pull values out of its response:

```
token = json .access_token
id    = json data.items.0.id
csrf  = header X-CSRF-Token
code  = regex /code=(\d+)/
```

What they find reaches every tab until binman quits, so a login in one tab
authorises the calls in the others. Rules belong to the tab and are not saved.

## Auth

The **Auth** section adds a bearer token, basic auth, an API key header, or
OAuth2 client credentials. An OAuth2 token is fetched before the request and
reused until 30 seconds before it expires, or for an hour when the endpoint
does not say. Fields whose names contain `secret` or `password` are masked.

## cURL

Paste a curl command into the URL bar and press `Enter` to import it; a
browser's "Copy as cURL" works. The import replaces the tab's request but keeps
its file, so `⌃S` afterwards overwrites that file. `⌃Y` shows the request as a
curl command, with variables resolved, and copies it with OSC 52.

## History

Every send is appended to `~/.local/state/binman/history.jsonl`, or under
`$XDG_STATE_HOME` when that is set. `⌃H` lists the last 50 and `Enter` sends
one again. Requests are stored with variables resolved, tokens included.

## Keys

| | |
| --- | --- |
| `⌃R` or `⌃J` | Send the request |
| `⌃C` | Cancel the request in flight |
| `⌃K` | Command palette |
| `⌃F` | Find a request anywhere in the collections |
| `⌃H` | History |
| `⌃E` | Environments |
| `⌃S` | Save the request; from the response pane, the response |
| `⌃Y` | Copy as cURL |
| `⌃T` / `⌃W` | New tab / close tab |
| `⌥1`…`⌥9`, `⌃PgUp` / `⌃PgDn` | Switch tabs |
| `Tab` / `⇧Tab` | Cycle panes |
| `⌥h` `⌥k` `⌥l` `⌥j` | Collections / URL / request / response |
| `F1` | Help, or `?` wherever it would not be typed |
| `F5` | Reload the selected folder from disk |
| `⌃Q` | Quit |

In the tree, `j`/`k` move, `l`/`Space` open or close a folder, `h` collapses,
`Enter` opens a request and `r` reloads. In the URL bar, `↑`/`↓` change the
method. In the request, `[`/`]` switch sections, `Enter` edits, `a` adds a row,
`d` deletes one, and `←`/`→` change the body or auth kind. In the response,
`[`/`]` switch views and `j`/`k`, `⌃D`/`⌃U`, `g`/`G` scroll. `Esc` stops
editing, or goes back to the collections.

The mouse works too: click a pane, tab, section, tree row, the environment
picker, the method, or **Send** at the end of the URL bar, which becomes
**Cancel** while a request is out. Clicking a selected row edits it, the wheel
scrolls, and a middle click closes a tab. Hold Shift (Option in iTerm2) to
select text.

Opening a request reuses the current tab unless it has edits (`•`) or a request
in flight (`◐`). Auth values, extraction rules and typed variables do not count
as edits, and are lost when the tab is reused.

## Design

Two crates: `binman-core` reads the formats, finds environments, resolves
variables and sends; `binman` is the terminal front end. Every send opens its
own connection, so its timing trace is its own, while the cookie jar is shared.
Redirects are followed, system proxy settings are honoured, and certificates
are checked by the operating system's verifier.

## Tests

```sh
cargo test --workspace
```

The HTTP client is tested against a real local server, and the front end by
driving the app against that server and a collection on disk and checking the
rendered screen.

## Not done yet

- A command mode for scripts, such as `binman send`.
- Reading or writing auth settings in request files.
- The Homebrew tap has not been updated for this version.

## Licence

Free to use, copy, modify and distribute for personal, educational and
non-commercial purposes. See [LICENSE](LICENSE).
