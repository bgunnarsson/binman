# binman

An HTTP client for the terminal. One directory of request files — `.http`,
Bruno `.bru`, `.graphql`, Postman collections, OpenAPI specs — as a tree you can
walk, requests open in tabs, and the response beside them, without leaving the
keyboard.

Version 2 is a rewrite in Rust. The Go implementation is the 1.x releases, the
last of them [1.0.8](https://github.com/bgunnarsson/binman/releases/tag/1.0.8);
see [Status](#status) for what behaves differently and what has not been
carried across.

```
 ✻  users/list.http  ·  staging                                                  https://api.staging.io
╭─ Collections ───── api ─╮ list.http   +
│ ▸  auth                 │╭─ Request ─────────────────────────────────────────────────────── staging ─╮
│ ▾  users  2             ││ GET {{BASE}}/users?page=2                                                 │
│     POST   create.bru   │╰───────────────────────────────────────────────────────────────────────────╯
│▌    GET    list.http    │╭─ Params 1  Headers 1  Body  Auth  Vars 1  Scripts  Info ──────────────────╮
│ ▸  petstore.yaml        ││ ▌page      2                                                              │
│                         ││  + add a parameter                                                        │
│                         ││                                                                           │
│                         │╰───────────────────────────────────────────────────────────────────────────╯
│                         │╭─ Body  Headers 4  Cookies  Scripts  Trace ────── 200 OK · 0.90ms · 119 B ─╮
│                         ││ {                                                                         │
│                         ││   "page": 2,                                                              │
│                         ││   "users": [                                                              │
│                         ││     {                                                                     │
│                         ││       "id": 41,                                                           │
│                         ││       "name": "Portishead",                                               │
│                         ││       "active": true                                                      │
│                         ││     },                                                                    │
╰─────────────────────────╯╰───────────────────────────────────────────────────────────────────────────╯
 200 OK in 0.90ms · 119 B                           ⇥ panes · ⌃R send · ⌃K commands · F1 help · ⌃Q quit
```

binman opens on a splash with the version, where the collections are, and the
three keys worth knowing. Any key dismisses it.

## Install

Each release carries a build for macOS, Linux and Windows on
[the releases page](https://github.com/bgunnarsson/binman/releases), with a
`checksums.txt` beside them:

```sh
tar -xzf binman-2.0.0-darwin-arm64.tar.gz
sudo mv binman-2.0.0-darwin-arm64/binman /usr/local/bin/
```

Or from source:

```sh
cargo build --release
# the binary lands at target/release/binman
```

Rust 1.90 or newer to build, and a Nerd Font in your terminal either way —
binman draws the tree with the same glyphs binvim and binsql do. Without one
the icons render as boxes; nothing else is affected.

## Configure

binman reads `~/.config/binman/config`, or `$XDG_CONFIG_HOME/binman/config`
when that is set. **A v1 config opens unchanged.**

```
# The directory holding your request files (required)
HTTP_FILES  = /path/to/your/collections

# How long to wait for a response. 30s when left out; 0 for no limit at all,
# which is what a streaming endpoint needs.
TIMEOUT     = 30s

# A client certificate for mTLS. Both must be set to take effect.
CLIENT_CERT = /path/to/client.crt
CLIENT_KEY  = /path/to/client.key
```

`HTTP_FILES` is required, and binman says where to put it when it is missing.
`TIMEOUT` takes a Go-style duration — `30s`, `2m`, `1m30s`, `500ms` — and one
that does not parse is reported rather than quietly replaced.

## Collection formats

binman recurses into `HTTP_FILES` and shows everything it can open as a tree.
Hidden files are left out.

### `.http`

```http
POST https://api.example.com/users
Content-Type: application/json
Authorization: Bearer {{TOKEN}}

{ "name": "Jane" }
```

The request line, headers until a blank line, then the body. Files written for
VS Code or JetBrains read too: comments before the request line, a trailing
`HTTP/1.1`, a bare URL meaning GET, and `###` ending the request. Only the first
request in a file is read.

### `.bru` (Bruno)

Bruno's block format, with its body modes — JSON, XML, text, SPARQL,
form-urlencoded, multipart and GraphQL. `collection.bru` and `folder.bru` are
read for shared variables and kept out of the tree, as is a collection's
`environments/` directory.

Saving a `.bru` rewrites only the blocks that changed. `meta`, `vars`, `auth`,
`script`, `docs` and any header Bruno has switched off with `~` are left
exactly as they were, and a request saved without edits leaves the file byte
for byte.

### `.graphql`

A GraphQL operation, posted as the JSON document servers expect. A header block
and a variables block are optional:

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

### Postman collections

A `*.postman_collection.json` export opens as its folders and requests, with
raw, urlencoded, form-data and GraphQL bodies, and the collection's `variable`
list as the lowest layer of variables.

### OpenAPI and Swagger

Any `.json`, `.yaml` or `.yml` whose start declares `openapi` or `swagger` opens
as its operations, grouped by tag:

```
▾  petstore.yaml
  ▾  pets  3
      GET    /pets
      POST   /pets
      GET    /pets/{id}
```

Opening an operation fills in the method and URL. Path parameters become
`{{placeholders}}`, query and header parameters are listed empty to fill in, and
a JSON request body sets `Content-Type`. The URL starts from the first
`servers` entry, from Swagger 2's `host` and `basePath`, or from `{{URL}}` when
the spec names neither — so an environment can say where the API lives.

## Environments and variables

`{{NAME}}` can appear anywhere in the URL, a header, the body or an auth field.

Environments are found by walking up from the request's directory to the
collection root:

```
collections/
  .env                  ← "default", for everything below
  .env.staging          ← "staging"
  cms/
    .env                ← a nearer "default" wins for requests in here
    delivery/
      get.http
```

Bruno `environments/*.bru` files and Postman `*.postman_environment.json` files
are found the same way and listed alongside. The one in use sits in the picker
at the right of the header; `⌃E` or a click drops its list: `Enter` uses an environment,
`e` opens its file to edit in place. The choice carries to every
request opened after it, so a session can be spent against staging without
choosing it each time. The file is read again before every send, so an edit
made outside binman is picked up.

Values in `.env` files may be quoted; the quotes are not part of the value. A
value may itself use a variable — `BASE = https://{{HOST}}/v1` — and is
expanded.

### Which value wins

Lowest to highest:

1. Collection vars — Postman `variable[]`, Bruno `collection.bru` and `folder.bru`
2. The selected environment
3. Vars the request declares itself — Bruno `vars:pre-request`
4. Vars extracted from earlier responses
5. Values typed into the request's **Vars** section

The **Vars** section lists every variable the request uses, its value, and which
layer it came from — or that nothing sets it. A variable only counts as typed
once you have typed it: opening a request does not pin anything, so a token
extracted after you opened it is still the one it sends. `d` hands a variable
back to the layers below.

A URL that still names an unset variable is not sent. It is lit in a warning
colour in the URL bar before you try, and the right end of the URL bar says
which host the rest of it resolves to.

### Extracting values from responses

Rules in a request's **Scripts** section pull values out of its response:

```
token = json .access_token
id    = json data.items.0.id
csrf  = header X-CSRF-Token
code  = regex /code=(\d+)/
```

What a rule finds is available to every request in every tab from then on,
which is how a login in one tab authorises the calls in the others.

## Auth

The **Auth** section adds one of:

- **Bearer token**
- **Basic auth**
- **API key** — any header name and value
- **OAuth2 client credentials** — the token is fetched before the request,
  reused until thirty seconds before it expires, and assumed to last an hour
  when the endpoint does not say. A refusal is reported with the endpoint's own
  answer and is never cached.

## cURL

- **Import**: paste a curl command into the URL bar and press `Enter`. Commands
  copied across several lines paste whole, and a browser's "Copy as cURL" —
  `$'…'` quoting, `-b` cookies and all — reads as it should.
- **Export**: `⌃Y` shows the request as a curl command and sends it to the
  clipboard with OSC 52, in the terminals that allow it. A multipart body goes
  out as `-F` fields.

## Keys

| | |
| --- | --- |
| `⌃R` or `⌃J` | Send the request |
| `⌃C` | Cancel the request in flight |
| `⌃K` | Command palette |
| `⌃F` | Find a request anywhere in the collections |
| `⌃H` | History — `Enter` sends one again |
| `⌃E` | Environments — `Enter` uses one, `e` edits its file |
| `⌃S` | Save the request to its file; from the response pane, the response |
| `⌃Y` | Copy as cURL |
| `⌃T` / `⌃W` | New tab / close tab |
| `⌥1`…`⌥9` | Jump to a tab |
| `Tab` / `⇧Tab` | Cycle panes |
| `⌥h` `⌥k` `⌥l` `⌥j` | Collections / URL / request / response |
| `F1` or `?` | Help |
| `F5` | Reload the selected folder from disk |
| `⌃Q` | Quit |

In the tree: `j`/`k` to move, `l`/`Space` to expand, `h` to collapse or step up,
`Enter` to open a request, `r` to reload. In the URL bar: `↑`/`↓` change the
method and `Enter` sends. In the request: `[`/`]` move between sections,
`Enter` edits, `a` adds, `d` deletes, `←`/`→` change the body or auth kind, and
`Esc` stops editing. In the response: `[`/`]` move between body, headers,
cookies, extracted values and the trace, and `j`/`k`, `⌃D`/`⌃U`, `g`/`G` scroll.

The mouse reaches the same things. A click focuses a pane, picks a tab, a
section or a response view, and opens a request in the tree; a click on the row
already selected edits it, as `Enter` would. The picker in the header drops the
environment list, a click on the method changes it, and **Send** at the end of
the URL bar sends — or cancels while a request is out. The status line's hints
can be clicked too. The wheel scrolls whatever is under the pointer, and a
middle click closes a tab. Holding Shift — Option in iTerm2 — still selects
text the terminal's own way.

Opening a request reuses the tab you are in unless it holds edits or a request
in flight, in which case it gets a new one. A request already open in a tab is
switched to rather than opened twice. A tab with edits carries `•` in the tab
strip.

## Design

Two crates, as in binsql:

- **`binman-core`** — reading the formats into one `Request`, finding
  environments, resolving variables, and sending. Nothing above it parses a
  file or opens a socket. It is a library so a command mode can be added over
  the same code.
- **`binman`** — the terminal front end: `app` holds the state and sends
  requests off the UI thread, `ui` draws it. A response is matched to its tab
  and its send by id, so one that arrives after you have moved on is dropped
  rather than landing in the wrong place.

Every send opens its own connection. It costs a handshake per request, which an
interactive client can afford, and buys a trace that is always about this
request: DNS and connect are timed on the connection this request used, so two
tabs sending at once cannot trade timings. The cookie jar is shared, so a login
followed by a call behaves as it would in a browser. Certificates are checked
by the operating system's verifier, so a certificate your system trusts — a
local development certificate in the keychain, say — is trusted here too.

### Look

The panels sit where v1 put them: the header with the environment picker at its
right, the URL bar across the whole width, then the collections beside the
request over the response, split two to five. The styling is binsql's, which
binman sits beside in the same terminal:

- **Two surfaces.** The body — the URL, the request, the response — is
  `#1e1e2e`; chrome — the tree, the tab strip, the header and status lines,
  every overlay — is `#181825`, so chrome reads as layered above.
- **binvim's chrome roles**, same names and values. `theme.rs` is the only file
  that names a colour; everything else asks for a role.
- **binvim's popup form.** Title after a single dash in the top border, a
  counter at the right end of the same border, and `▌` down the left of the
  selected row. The request and response panes carry their sections in their
  top border.
- **A scrim behind open modals**, so the modal is plainly the thing being
  talked to.
- **The header and status line follow Claude Code**: one mark in its coral,
  then plain text separated by `·`.

## Tests

```sh
cargo test --workspace
```

The core is tested against a real socket: a small HTTP server in the test
suite answers each test's requests, so cookies, cancelling, event streams,
timeouts and the OAuth2 token cache are exercised end to end rather than
mocked. The front end is tested by driving the real application against that
server and a real collection on disk, and asserting on the rendered screen —
including that the header and status line stay legible and that every modal is
sized to what it holds.

## Status

### What behaves differently from v1

Some of these fix v1 bugs; the rest follow binsql.

- **Bruno blocks close where Bruno closes them.** v1 ended a block at any line
  that trimmed to `}`, so a JSON body holding a nested object ran on to the end
  of the file. Saving a `.bru` no longer rewrites the whole file from four
  fields.
- **Only typed values override.** v1 filled the Vars tab when a request opened
  and let every value in it override, so a token extracted afterwards was
  hidden behind the empty one filled in before it existed.
- **Editing a query parameter no longer re-encodes the URL.** v1 turned
  `{{id}}` into `%7B%7Bid%7D%7D` on every edit in the Params tab, and it
  stopped resolving.
- **`TIMEOUT = 0` means no timeout**, as v1's README said. v1's code applied
  30 seconds anyway.
- **`⌃C` only cancels; `⌃Q` quits**, as in binsql. `⌃T` opens a tab, so the
  method moved to `↑`/`↓` in the URL bar. The environment dropdown opens on
  `⌃E`, and editing the environment file is `e` in its list.
- **Auth kinds that only pretended are gone.** v1 listed AWS Sig v4, Digest,
  NTLM, WSSE and a bare "OAuth 2.0" and sent their field labels as literal
  headers. "Inherit" did nothing either. The **Options** tab never did anything
  and is gone; **Info** now says where a request came from and where it goes.
- **Header order is kept** and a header may repeat. v1 kept headers in a map.
- **The trace times TCP and TLS together**, because the HTTP library does them
  in one step.

### Not carried across yet

- **The Homebrew tap.** Releases are built by GitHub Actions when a `v*` tag
  is pushed, but the formula in `bgunnarsson/homebrew-binman` is still v1's: it
  builds 1.0.8 from source with Go.
- **A command mode** — `binman send`, for scripts and agents, the way binsql
  has `query`, `exec` and `inspect`. The core is a library so it can be added.
- **Saving a request that has no file** — a new tab, a replay from history —
  to a new one.
- Reading or writing auth settings in request files. v1 did not either.

## Licence

Free to use, copy, modify and distribute for personal, educational and
non-commercial purposes. See [LICENSE](LICENSE).
