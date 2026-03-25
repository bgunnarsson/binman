# Plan: Bruno & Postman Collection Support

## Context
binman currently only loads `.http` files. Users want to open `.bru` (Bruno) and `.postman_collection.json` (Postman) files from the sidebar. The goal is to parse each format into the existing `httpfile.Request` struct so the rest of the app (execution, env vars, UI) works unchanged.

**Key constraint:** Bruno is 1 file = 1 request (easy). Postman is 1 file = N nested requests (needs virtual tree nodes).

---

## Architecture Overview

The app's load path is:
```
file on disk → parser → *httpfile.Request → app.LoadFile → UI
```

All changes stay in the parsing and sidebar layers. Nothing in `httpclient`, `envfile`, or the UI tabs needs to change.

---

## Step 1 — `internal/brufile/` (new package)

**`parse.go`**: Parse `.bru` block syntax into `*httpfile.Request`.

The format is block-based:
```
get {
  url: https://api.example.com
}
headers {
  Content-Type: application/json
}
body:json {
  {"key": "value"}
}
```

- Method: the block keyword before `{` at the top level (`get`, `post`, `put`, `patch`, `delete`, `head`)
- URL: `url:` line inside the method block
- Headers: key-value lines inside `headers {}` block
- Body: content inside `body:json {}`, `body:text {}`, `body:xml {}` etc.

**`load.go`**: `Load(path string) (*httpfile.Request, error)` — reads file, calls `Parse`.

---

## Step 2 — `internal/postmanfile/` (new package)

**`model.go`**: Minimal Go structs for Postman Collection v2.1 JSON:
```go
type Collection struct {
    Info  Info   `json:"info"`
    Items []Item `json:"item"`
}
type Item struct {
    Name    string   `json:"name"`
    Request *Request `json:"request"` // nil if folder
    Items   []Item   `json:"item"`    // non-nil if folder
}
type Request struct {
    Method string     `json:"method"`
    URL    PostmanURL `json:"url"`
    Header []Header   `json:"header"`
    Body   *Body      `json:"body"`
}
type PostmanURL struct {
    Raw string `json:"raw"` // also handle plain string via UnmarshalJSON
}
type Header struct {
    Key      string `json:"key"`
    Value    string `json:"value"`
    Disabled bool   `json:"disabled"`
}
type Body struct {
    Mode string `json:"mode"`
    Raw  string `json:"raw"`
}
```

Note: Postman `url` can be a plain string OR an object — handle with a custom `UnmarshalJSON`.

**`parse.go`**: `Parse(data []byte) (*Collection, error)` via `json.Unmarshal`.

**`extract.go`**: `RequestAt(c *Collection, itemPath []int) (*httpfile.Request, error)` — walks the item tree by index path `[0, 2, 1]` and maps the matched `Item.Request` into `*httpfile.Request`.

---

## Step 3 — `internal/fsview/tree.go`

### New node kinds
```go
const (
    NodeDir NodeKind = iota
    NodeHTTPFile
    NodeOtherFile
    NodeBruFile           // .bru (Bruno)
    NodePostmanCollection // .postman_collection.json (whole file, expandable)
    NodePostmanRequest    // virtual: one request inside a Postman collection
)
```

### New reference type for virtual Postman request nodes
```go
type PostmanNode struct {
    CollectionPath string
    ItemPath       []int  // index path into nested items
    Name           string
    Method         string
}
```
Used as the tview node reference instead of `FSNode`.

### `Handlers` — add new callback
```go
type Handlers struct {
    OpenHTTPFile       func(path string)
    OpenPostmanRequest func(collectionPath string, itemPath []int)
}
```

### `populateDir` — detect new file types
- `.bru` → `NodeBruFile` (same treatment as `NodeHTTPFile`)
- name ends with `.postman_collection.json` → `NodePostmanCollection`

### `activateNode` — handle new kinds
- `NodeBruFile` → call `h.OpenHTTPFile(ref.Path)` (same handler, app routes by extension)
- `NodePostmanCollection` → parse collection, populate virtual child nodes (`PostmanNode` references), expand
- `PostmanNode` reference → call `h.OpenPostmanRequest(node.CollectionPath, node.ItemPath)`

### `peekBruMethod(path string) string`
Read `.bru` file, find first block keyword (`get`, `post`, etc.) and return uppercased method.

### Labels
- `.bru` files: reuse existing `httpFileLabelWithMethod` — same method colors
- Postman collection node: folder-style label with a distinct icon
- Postman request child nodes: `httpFileLabelWithMethod(method, name)`

---

## Step 4 — `internal/app/actions.go`

### Update `LoadFile`
Route by file extension:
```go
func (a *App) LoadFile(path string) {
    var req *httpfile.Request
    var err error
    switch strings.ToLower(filepath.Ext(path)) {
    case ".bru":
        req, err = brufile.Load(path)
    default: // .http
        req, err = httpfile.Load(path)
    }
    // rest unchanged ...
}
```

### Add `LoadPostmanRequest`
```go
func (a *App) LoadPostmanRequest(collectionPath string, itemPath []int) {
    data, err := os.ReadFile(collectionPath)
    // parse collection, call postmanfile.RequestAt, update view
    a.State.CurrentRequest = req
    a.View.UpdateRequestView(req)
    // env discovery uses collection file's directory
}
```

---

## Step 5 — `internal/app/app.go`

Wire up the new handler when creating the fsview tree:
```go
fsview.Handlers{
    OpenHTTPFile:       a.LoadFile,
    OpenPostmanRequest: a.LoadPostmanRequest,
}
```

---

## Files to modify

| File | Change |
|------|--------|
| `internal/fsview/tree.go` | New node kinds, PostmanNode struct, populate/activate logic, peek methods |
| `internal/app/actions.go` | Route LoadFile by extension, add LoadPostmanRequest |
| `internal/app/app.go` | Wire OpenPostmanRequest handler |

## Files to create

| File | Purpose |
|------|---------|
| `internal/brufile/parse.go` | Bruno `.bru` parser |
| `internal/brufile/load.go` | File I/O for Bruno |
| `internal/postmanfile/model.go` | Postman JSON structs |
| `internal/postmanfile/parse.go` | JSON unmarshal |
| `internal/postmanfile/extract.go` | Walk item tree → `httpfile.Request` |

---

## Verification

1. `go build ./...` — must compile cleanly
2. Place a `.bru` file in the collection root → appears in sidebar with method badge, loads into editor on click, sends successfully
3. Place a `.postman_collection.json` in the collection root → appears as expandable node, child requests visible with method badges, click loads request, sends successfully
4. Existing `.http` files continue to work unchanged
5. Nested Postman folders expand correctly
