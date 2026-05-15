# binman

A terminal UI HTTP client. Browse and send HTTP requests from your terminal without leaving the keyboard.

Supports `.http`, `.bru` (Bruno), `.graphql`, `.postman_collection.json` (Postman), and OpenAPI/Swagger spec files.

---

## Features

- Sidebar file browser — navigate collections as a directory tree
- Tabs for params, headers, vars, body, auth, info, scripts, options
- Multiple body types: raw, JSON, form-urlencoded, multipart
- Auth: Basic, Bearer, API Key, OAuth2 client credentials
- Syntax-highlighted JSON responses, with streaming for SSE/chunked endpoints
- Environment variable support via `.env`, Bruno `environments/*.bru`, and `*.postman_environment.json`
- Post-response variable extraction (chain requests from extracted tokens, IDs, etc.)
- Fuzzy search across every request in your collections
- Request history with one-key replay
- Import / export `curl` commands
- Save edited requests back to disk; save responses to file
- Cycle HTTP methods without touching the mouse
- Works on macOS, Linux, and Windows

---

## Installation

### macOS — Homebrew

```sh
brew tap bgunnarsson/binman
brew install binman
```

### macOS / Linux — pre-built binary

Download the latest release for your platform from the [releases page](https://github.com/bgunnarsson/binman/releases):

| Platform        | File                              |
|-----------------|-----------------------------------|
| macOS (Apple Silicon) | `binman-darwin-arm64`       |
| macOS (Intel)   | `binman-darwin-amd64`             |
| Linux (x86_64)  | `binman-linux-amd64`              |
| Linux (ARM64)   | `binman-linux-arm64`              |

Then make it executable and move it to your PATH:

```sh
chmod +x binman-darwin-arm64
mv binman-darwin-arm64 /usr/local/bin/binman
```

### Windows — pre-built binary

Download `binman-windows-amd64.exe` from the [releases page](https://github.com/bgunnarsson/binman/releases), rename it to `binman.exe`, and place it somewhere on your `PATH`.

### Build from source

Requires Go 1.22+.

```sh
git clone https://github.com/bgunnarsson/binman.git
cd binman
go build -o binman ./cmd/binman
```

---

## Configuration

binman reads `~/.config/binman/config` on startup. On systems with `$XDG_CONFIG_HOME` set, it uses `$XDG_CONFIG_HOME/binman/config` instead.

Create the file:

```sh
mkdir -p ~/.config/binman
```

**`~/.config/binman/config`**

```
# Path to the directory containing your HTTP request files (required)
HTTP_FILES  = /path/to/your/collections

# Default per-request timeout. Omit or set to 0 for no timeout
# (useful for streaming endpoints). Accepts any Go duration string.
TIMEOUT     = 30s

# Optional client certificate for mTLS. Both must be set to take effect.
CLIENT_CERT = /path/to/client.crt
CLIENT_KEY  = /path/to/client.key
```

`HTTP_FILES` is required. binman will refuse to start without it.

---

## Collection formats

Point `HTTP_FILES` at any directory. binman will recurse into subdirectories and display all supported files in the sidebar.

### `.http` files

Plain text, one request per file:

```http
POST https://api.example.com/users
Content-Type: application/json
Authorization: Bearer {{TOKEN}}

{
  "name": "Jane"
}
```

The first line is `METHOD URL`. Headers follow until a blank line. Everything after the blank line is the body.

### `.bru` files (Bruno)

Bruno's block-based format:

```
get {
  url: https://api.example.com/users
}

headers {
  Authorization: Bearer {{TOKEN}}
}
```

`collection.bru` and `folder.bru` files are picked up automatically for shared variables.

### `.graphql` files

Plain GraphQL operation files. binman wraps the query in a JSON body and sets `Content-Type: application/json` automatically.

### `.postman_collection.json` files (Postman)

Drop a Postman collection export into your collection directory. binman will expand it in the sidebar as a tree of folders and requests.

### OpenAPI / Swagger specs

Drop an OpenAPI 3.x or Swagger 2.x spec (YAML or JSON) into your collection directory. binman detects it by content and expands it in the sidebar grouped by tag:

```
▶ openapi.yaml
  ▼ Pets
    GET  /pets
    POST /pets
    GET  /pets/{id}
  ▼ Orders
    GET  /orders
```

Selecting an operation loads the URL and method. A `Content-Type: application/json` header is added automatically when the operation defines a JSON request body. Path parameters like `{id}` can be filled in manually or via environment variables.

Both `.yaml`/`.yml` and `.json` spec files are supported.

---

## Environment variables

Place a `.env` file anywhere in your collection tree to define variables. binman walks up from the request file's directory to the root, so a single `.env` at the top of your collections folder applies to all requests within it:

```
collections/
  .env                  ← applies to everything below
  cms/
    delivery/
      get.http          ← picks up collections/.env
```

A more specific `.env` in a subdirectory takes precedence over one higher up. Use `.env.*` files for multiple environments:

```
.env              → labeled "default"
.env.staging      → labeled "staging"
.env.production   → labeled "production"
```

Bruno `environments/*.bru` files and Postman `*.postman_environment.json` files are also discovered automatically and appear in the same dropdown.

Switch between environments using the dropdown in the URL bar. Press `Ctrl-E` to edit the selected env file in-place.

```sh
# .env
BASE_URL = https://api.example.com
TOKEN    = my-secret-token
```

Use `{{VAR_NAME}}` anywhere in the URL, headers, or body:

```http
GET {{BASE_URL}}/users
Authorization: Bearer {{TOKEN}}
```

### Variable precedence

Lowest to highest:

1. Collection vars (Postman `variable[]`, Bruno `collection.bru` / `folder.bru`)
2. Selected env source (`.env`, Bruno env, Postman env)
3. Request-level vars (e.g. Bruno `vars:pre-request`)
4. Vars extracted from previous responses (see Scripts)
5. Manual overrides from the **Vars** tab

### Extracting variables from responses

The **Scripts** tab on the request panel accepts simple extraction rules. After a response comes back, matching values are stored and become available to later requests as `{{name}}`:

```
token = json .access_token
id    = json .user.id
csrf  = header X-CSRF-Token
```

---

## cURL interop

- **Import**: paste a `curl ...` command into the URL bar and press `Enter`. binman parses the method, URL, headers, and body into the request panel.
- **Export**: press `Ctrl-Y` to render the current request as a `curl` one-liner in the response area, ready to select-and-copy.

---

## Keyboard shortcuts

| Key       | Action                                              |
|-----------|-----------------------------------------------------|
| `Ctrl-J`  | Send request                                        |
| `Ctrl-C`  | Cancel in-flight request, otherwise quit            |
| `Ctrl-Q`  | Quit                                                |
| `Ctrl-T`  | Cycle HTTP method                                   |
| `Ctrl-S`  | Save request to source file (or response, if focused) |
| `Ctrl-Y`  | Copy current request as `curl`                      |
| `Ctrl-F`  | Fuzzy-search every request in the collection        |
| `Ctrl-H`  | Open history; Enter to replay                       |
| `Ctrl-E`  | Edit the selected env file                          |
| `Tab`     | Cycle focus through panels                          |
| `[` / `]` | Previous / next tab in the focused panel            |
| `Escape`  | Focus sidebar                                       |
| `Enter`   | Open file / expand directory                        |

Focus cycles: Sidebar → URL bar → Send button → Request panel → Response panel → Sidebar.
