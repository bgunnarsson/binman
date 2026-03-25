# binman

A terminal UI HTTP client. Browse and send HTTP requests from your terminal without leaving the keyboard.

Supports `.http`, `.bru` (Bruno), `.postman_collection.json` (Postman), and OpenAPI/Swagger spec files.

---

## Features

- Sidebar file browser — navigate collections as a directory tree
- Tabs for request headers, body, params, auth
- Syntax-highlighted JSON responses
- Environment variable support via `.env` files
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
# Path to the directory containing your HTTP request files
HTTP_FILES = /path/to/your/collections
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

Switch between environments using the dropdown in the URL bar.

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

---

## Keyboard shortcuts

| Key       | Action                              |
|-----------|-------------------------------------|
| `Ctrl-J`  | Send request                        |
| `Ctrl-T`  | Cycle HTTP method                   |
| `Escape`  | Focus sidebar                       |
| `Tab`     | Cycle focus through panels          |
| `Enter`   | Open file / expand directory        |
| `Ctrl-C`  | Quit                                |
| `Ctrl-Q`  | Quit                                |

Focus cycles: Sidebar → URL bar → Send button → Request panel → Response panel → Sidebar.
