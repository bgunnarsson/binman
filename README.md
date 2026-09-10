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
 ✻  binman                                                                                               ⌃E  staging ▾
 list.http   +
╭─ Request ─────────────────────────────────────────────────────────────────────────────────── https://api.staging.io ─╮
│ GET {{BASE}}/users?page=2                                                                                      Send  │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ Collections ────────────────────────── api ─╮╭─ Params 1  Headers 1  Body  Auth  Vars 1  Scripts  Info ─────────────╮
│ ▸   auth                                     ││ ▌page      2                                                         │
│ ▾   users  2                                 ││  + add a parameter                                                   │
│     POST   create.bru                        │╰──────────────────────────────────────────────────────────────────────╯
│▌    GET    list.http                         │╭─ Body  Headers 4  Cookies  Scripts  Trace ─ 200 OK · 0.90ms · 119 B ─╮
│ ▸   petstore.yaml                            ││ {                                                                    │
│                                              ││   "page": 2,                                                         │
│                                              ││   "users": [                                                         │
│                                              ││     {                                                                │
│                                              ││       "id": 41,                                                      │
│                                              ││       "name": "Portishead",                                          │
│                                              ││       "active": true                                                 │
│                                              ││     },                                                               │
│                                              ││     {                                                                │
╰──────────────────────────────────────────────╯╰──────────────────────────────────────────────────────────────────────╯
 200 OK in 0.90ms · 119 B                                           ⇥ panes · ⌃R send · ⌃K commands · F1 help · ⌃Q quit
```

binman opens on a splash with the version, where the collections are, and the
three keys worth knowing. Any key or click dismisses it.

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
binman draws the tree's icons from one, as binvim and binsql do. Without one
the icons render as boxes; nothing else is affected.

`--version` (`-V`) and `--help` (`-h`) are the only arguments it takes.

## Configure

binman reads `~/.config/binman/config`, or `$XDG_CONFIG_HOME/binman/config`
when that is set. **A v1 config opens unchanged**, unless its `TIMEOUT` is `0`
or does not parse — see [Status](#status).

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
It has to be a directory that exists; a relative path is taken from where
binman is started. `TIMEOUT` takes a Go-style duration — `30s`, `2m`, `1m30s`,
`500ms` — and one that does not parse stops binman with an error rather than
being quietly replaced. The certificate and key are read at start, so a missing
or mismatched pair is reported then rather than on the first send.

Values are taken as written: quotes are part of the value and `~` is not
expanded. A key binman does not know is skipped without a word, so a misspelled
one simply has no effect.

## Collection formats

binman recurses into `HTTP_FILES` and shows everything it can open as a tree,
directories first. Hidden files and directories are left out, symbolic links
are followed, and a file that does not parse shows its error in the tree.

### `.http`

```http
POST https://api.example.com/users
Content-Type: application/json
Authorization: Bearer {{TOKEN}}

{ "name": "Jane" }
```

The request line, headers until a blank line, then the body. Files written for
VS Code or JetBrains read too: `#` and `//` comments before the request line, a
trailing `HTTP/1.1`, a bare `https://…` URL meaning GET, and `###` ending the
body. Only the first request in a file is read. A body with no `Content-Type`
is sent as JSON.

Saving a `.http` writes the file afresh from the request, so comments and any
requests after the first are not kept. `.http` and `.bru` are the formats that
save; requests from the others can be edited and sent, but not saved.

### `.bru` (Bruno)

Bruno's block format, with its body modes — JSON, XML, text, SPARQL,
form-urlencoded, multipart and GraphQL. A multipart `@file(…)` field uploads
the file, relative to the request. `collection.bru` and `folder.bru` are read
for shared variables and kept out of the tree, as is the `environments/`
directory beside a `collection.bru`. Headers switched off with `~` are not
sent, and scripts, tests and asserts are not run.

Saving a `.bru` rewrites only the blocks that changed. `meta`, `vars`, `auth`,
`script` and `docs` are left exactly as they were, headers switched off with
`~` are kept after the ones in use, and a request saved without edits leaves
the file byte for byte.

### `.graphql`

A GraphQL operation, posted as the JSON document servers expect. The header
block is optional, and a variables block may follow the query when the header
block is there:

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
a JSON request body sets `Content-Type` — when they are written out in the
spec; a `$ref` to a parameter or a body is not followed. The URL starts from
the first `servers` entry, from Swagger 2's `host` and `basePath`, or from
`{{URL}}` when the spec names neither — so an environment can say where the API
lives. A relative server URL is added to `{{URL}}`.

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

A nearer file replaces the one above it rather than adding to it. Postman
`*.postman_environment.json` files are found the same way, and Bruno
`environments/*.bru` files from the nearest `environments/` directory; all of
them are listed together. Bruno's `vars:secret` entries are not read.

The environment in use sits in the picker at the right of the header; `⌃E` or a
click drops its list: `Enter` uses an environment, `e` opens its file to edit
in place, where `⌃S` saves and `Esc` discards. The choice carries by name to
every request opened after it, so a session can be spent against staging
without choosing it each time. The file is read again before every send, so an
edit made outside binman is picked up.

Values in `.env` files may be quoted; the quotes are not part of the value. A
value may itself use a variable — `BASE = https://{{HOST}}/v1` — and is
expanded.

### Which value wins

Lowest to highest:

1. Collection vars — Postman `variable[]`, Bruno `collection.bru` and `folder.bru`
2. The selected environment
3. Vars the request declares itself — a `.bru` file's `vars` and
   `vars:pre-request`
4. Vars extracted from earlier responses
5. Values typed into the request's **Vars** section

The **Vars** section lists every variable written in the request, its value,
and which layer it came from — or that nothing sets it. A variable used only
inside another variable's value is not listed. A variable only counts as typed
once you have typed it: opening a request does not pin anything, so a token
extracted after you opened it is still the one it sends. `d` hands a variable
back to the layers below.

A URL that still names an unset variable is not sent or copied as curl. The
variable is lit in a warning colour in the URL bar before you try, and the
right end of the URL bar says which host the rest of it resolves to. An unset
variable in a header or the body goes out as written.

### Extracting values from responses

Rules in a request's **Scripts** section pull values out of its response:

```
token = json .access_token
id    = json data.items.0.id
csrf  = header X-CSRF-Token
code  = regex /code=(\d+)/
```

What a rule finds is available to every request in every tab from then on,
which is how a login in one tab authorises the calls in the others. The
response's **Scripts** view lists what was found, a rule that finds nothing
leaves the earlier value in place, and extracted values last until binman
quits. The rules belong to the tab: they are not saved to the request's file,
and a request opened into the tab starts without them.

## Auth

The **Auth** section adds one of:

- **Bearer token**
- **Basic auth**
- **API key** — any header name and value
- **OAuth2 client credentials** — the token is fetched before the request,
  reused until thirty seconds before it expires, and assumed to last an hour
  when the endpoint does not say. A refusal is reported with the endpoint's own
  answer and is never cached.

The auth header replaces a header of the same name in **Headers**. A field or
variable whose name says `secret` or `password` shows only its first five
characters.

## cURL

- **Import**: paste a curl command into the URL bar and press `Enter`. Commands
  copied across several lines paste whole, and a browser's "Copy as cURL" —
  `$'…'` quoting, `-b` cookies and all — reads as it should. Flags with no
  bearing on the request — `-k`, `-L`, `--compressed`, `--proxy` — are
  dropped, so certificates are still checked. The import replaces the request
  in the tab but keeps its file, so `⌃S` afterwards writes the import over it.
- **Export**: `⌃Y` shows the request as a curl command and sends it to the
  clipboard with OSC 52, in the terminals that allow it. Variables are
  resolved, so the command carries whatever secrets they hold; an OAuth2 token
  is left out. A multipart body goes out as `-F` fields.

## History

Every send is appended to `~/.local/state/binman/history.jsonl`, or
`$XDG_STATE_HOME/binman/history.jsonl` when that is set, in v1's format. `⌃H`
lists the last fifty, newest first, and `Enter` sends one again at once, in a
tab with no file behind it. A send that failed is kept and marked as failed; a
cancelled one is not kept.

What is recorded is the request as it went out, variables resolved — auth
headers and OAuth2 tokens included — so the file holds whatever secrets those
requests carried.

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
| `⌥1`…`⌥9` | Jump to a tab; `⌃PgUp` / `⌃PgDn` step through them |
| `Tab` / `⇧Tab` | Cycle panes |
| `⌥h` `⌥k` `⌥l` `⌥j` | Collections / URL / request / response |
| `F1` | Help — `?` too, wherever it would not be typed |
| `F5` | Reload the selected folder from disk |
| `⌃Q` | Quit |

In the tree: `j`/`k` to move, `l`/`Space` to open or close a folder, `h` to
collapse or step up, `Enter` to open a request, `r` to reload. In the URL bar:
`↑`/`↓` change the method and `Enter` sends. In the request: `[`/`]` move
between sections, `Enter` edits, `a` adds a row and `d` deletes one — in
**Auth** it clears the field, in **Vars** it hands the variable back — and
`←`/`→` change the body or auth kind. `Esc` stops editing, dropping a
half-typed cell. In the response: `[`/`]` move between body, headers, cookies,
extracted values and the trace, and `j`/`k`, `⌃D`/`⌃U`, `g`/`G` scroll. With
nothing being edited, `Esc` goes back to the collections.

The mouse reaches the same things. A click focuses a pane, picks a tab, a
section or a response view, and opens a request in the tree; a click on the row
already selected edits it, as `Enter` would. The picker in the header drops the
environment list, a click on the method moves it on to the next, a click in the
URL puts the cursor there, and **Send** at the end of the URL bar sends — or
cancels while a request is out. The status line's hints can be clicked too, all
but `⌃Q quit`. The wheel scrolls the response or moves through a list,
whichever is under the pointer, and a middle click closes a tab. Holding Shift
— Option in iTerm2 — still selects text the terminal's own way.

Opening a request reuses the tab you are in unless it holds edits or a request
in flight, in which case it gets a new one. Edits here means the method, URL,
headers or body: auth values, extraction rules and typed variables do not
count, and are dropped when another request opens into the tab. A request
already open in a tab is switched to rather than opened twice. In the tab
strip, `•` marks a tab with edits and `◐` one with a request out; `+` opens a
new tab, and the last tab does not close. Sending again from a tab cancels the
request it already has out.

## Design

Two crates, as in binsql:

- **`binman-core`** — reading the formats into one `Request`, finding
  environments, resolving variables, and sending. Nothing above it parses a
  file or opens a socket. It is a library so a command mode can be added over
  it, though preparing a request for sending — auth, the OAuth2 token, the
  body, extraction rules — still happens in the front end and would have to
  move down first.
- **`binman`** — the terminal front end: `app` holds the state and sends
  requests off the UI thread, `ui` draws it. A response is matched to its tab
  and its send by id, so one that arrives after you have moved on is dropped
  rather than landing in the wrong place.

Every send opens its own connection. It costs a handshake per request, which an
interactive client can afford, and buys a trace that is always about this
request: DNS and connect are timed on the connection this request used, so two
tabs sending at once cannot trade timings. The cookie jar is shared and kept in
memory, so a login followed by a call behaves as it would in a browser.
Redirects are followed, and the system's proxy settings are honoured.
Certificates are checked by the operating system's verifier, so a certificate
your system trusts — a local development certificate in the keychain, say — is
trusted here too.

### Look

The panels sit where v1 put them: the header with the environment picker at its
right, the URL bar across the whole width, then the collections beside the
request over the response, split two to five. The collections are 48 columns
wide, or two fifths of a narrower terminal, and v2's tab strip sits between the
header and the URL bar. The styling is binsql's, which binman sits beside in
the same terminal:

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
- **The header and status line follow Claude Code**: the header is one mark in
  its coral and the program's name, the status line plain text with its hints
  separated by `·`.

## Tests

```sh
cargo test --workspace
```

The core's client is tested against a real socket: a small HTTP server in the
test suite answers each test's requests, so cookies, cancelling, event streams,
timeouts and the OAuth2 token cache are exercised end to end rather than
mocked. The formats have unit tests of their own. The front end is tested by
driving the real application against that server and a real collection on
disk, and asserting on the rendered screen — including that the header and
status line stay legible and that every modal is sized to what it holds.

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
  30 seconds anyway. A `TIMEOUT` that does not parse is an error; v1 used 30
  seconds without saying so.
- **`⌃C` only cancels; `⌃Q` quits**, as in binsql. `⌃T` opens a tab, so the
  method moved to `↑`/`↓` in the URL bar. The environment dropdown opens on
  `⌃E`, and editing the environment file is `e` in its list.
- **Auth kinds that only pretended are gone.** v1 listed AWS Sig v4, Digest,
  NTLM, WSSE and a bare "OAuth 2.0" and sent their field labels as literal
  headers. "Inherit" did nothing either. The **Options** tab never did anything
  and is gone; **Info** now says where a request came from and where it goes.
- **Header order is kept** and a header may repeat. v1 kept headers in a map,
  and history still does, because it keeps v1's format: a replay sends them
  sorted, one of each.
- **The trace times TCP and TLS together**, because the HTTP library does them
  in one step.

### Not carried across yet

- **The Homebrew tap.** Releases are built by GitHub Actions when a `v*` tag
  is pushed, but the formula in `bgunnarsson/homebrew-binman` is still v1's: it
  builds 1.0.8 from source with Go.
- **A command mode** — `binman send`, for scripts and agents, the way binsql
  has `query`, `exec` and `inspect`. The core is a library so it can be added,
  once preparing a request moves down into it.
- **Saving a request that has no file** — a new tab, a replay from history —
  to a new one.
- Reading or writing auth settings in request files. v1 did not either.

## Licence

Free to use, copy, modify and distribute for personal, educational and
non-commercial purposes. See [LICENSE](LICENSE).
