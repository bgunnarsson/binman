package fsview

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/bgunnarsson/binman/internal/postmanfile"
	"github.com/gdamore/tcell/v2"
	"github.com/rivo/tview"
)

type NodeKind int

const (
	NodeDir NodeKind = iota
	NodeHTTPFile
	NodeOtherFile
	NodeBruFile
	NodePostmanCollection
	NodePostmanRequest
)

type FSNode struct {
	Kind NodeKind
	Path string
}

// PostmanNode is the tview reference for a virtual Postman request node.
type PostmanNode struct {
	CollectionPath string
	ItemPath       []int
	Name           string
	Method         string
}

// PostmanFolderNode is the tview reference for a virtual Postman folder node.
type PostmanFolderNode struct {
	Name string
}

type Filter struct {
	ShowNonHTTP bool
}

type Handlers struct {
	OpenHTTPFile       func(path string)
	OpenPostmanRequest func(collectionPath string, itemPath []int)
}

func NewTree(root string, f Filter, h Handlers) *tview.TreeView {
	root = filepath.Clean(root)

	rootNode := tview.NewTreeNode(dirLabel(iconExpanded(), filepath.Base(root))).
		SetReference(FSNode{Kind: NodeDir, Path: root}).
		SetExpanded(true).
		SetTextStyle(tcell.StyleDefault.Background(tcell.NewHexColor(0x0d0f1e))).
		SetSelectedTextStyle(tcell.StyleDefault.
			Background(tcell.NewHexColor(0x2d1f6e)).
			Foreground(tcell.ColorWhite))

	tree := tview.NewTreeView().
		SetRoot(rootNode).
		SetCurrentNode(rootNode)

	populateDir(rootNode, root, f)

	tree.SetGraphics(false)

	activateNode := func(n *tview.TreeNode) {
		if n == nil {
			return
		}

		// Virtual Postman request node
		if pn, ok := n.GetReference().(PostmanNode); ok {
			if h.OpenPostmanRequest != nil {
				h.OpenPostmanRequest(pn.CollectionPath, pn.ItemPath)
			}
			return
		}

		// Virtual Postman folder node
		if fn, ok := n.GetReference().(PostmanFolderNode); ok {
			n.SetExpanded(!n.IsExpanded())
			if n.IsExpanded() {
				n.SetText(dirLabel(iconExpanded(), fn.Name))
			} else {
				n.SetText(dirLabel(iconCollapsed(), fn.Name))
			}
			return
		}

		ref, ok := n.GetReference().(FSNode)
		if !ok {
			return
		}

		switch ref.Kind {
		case NodeDir:
			if len(n.GetChildren()) == 0 {
				populateDir(n, ref.Path, f)
			}
			n.SetExpanded(!n.IsExpanded())
			name := filepath.Base(ref.Path)
			if n.IsExpanded() {
				n.SetText(dirLabel(iconExpanded(), name))
			} else {
				n.SetText(dirLabel(iconCollapsed(), name))
			}

		case NodeHTTPFile, NodeBruFile:
			if h.OpenHTTPFile != nil {
				h.OpenHTTPFile(ref.Path)
			}

		case NodePostmanCollection:
			if len(n.GetChildren()) == 0 {
				populatePostmanCollection(n, ref.Path)
			}
			n.SetExpanded(!n.IsExpanded())
			name := filepath.Base(ref.Path)
			if n.IsExpanded() {
				n.SetText(postmanCollectionLabel(iconExpanded(), name))
			} else {
				n.SetText(postmanCollectionLabel(iconCollapsed(), name))
			}
		}
	}

	// --- Keyboard: Enter activates immediately ---
	tree.SetInputCapture(func(ev *tcell.EventKey) *tcell.EventKey {
		if isEnter(ev) {
			activateNode(tree.GetCurrentNode())
			return nil
		}
		return ev
	})

	// SetSelectedFunc fires after tview updates the current node, so the node
	// passed here is always the one the user actually clicked or activated.
	// Safe to call directly — runs on the main goroutine without QueueUpdate.
	tree.SetSelectedFunc(func(node *tview.TreeNode) {
		activateNode(node)
	})

	return tree
}

func populateDir(parent *tview.TreeNode, dir string, f Filter) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return
	}

	type item struct {
		name string
		path string
		kind NodeKind
	}
	var items []item

	for _, e := range entries {
		name := e.Name()
		if strings.HasPrefix(name, ".") {
			continue // skip hidden files and dirs
		}
		path := filepath.Join(dir, name)

		if e.IsDir() {
			items = append(items, item{name, path, NodeDir})
			continue
		}

		if strings.EqualFold(filepath.Ext(name), ".http") {
			items = append(items, item{name, path, NodeHTTPFile})
			continue
		}

		if strings.EqualFold(filepath.Ext(name), ".bru") {
			items = append(items, item{name, path, NodeBruFile})
			continue
		}

		if strings.HasSuffix(strings.ToLower(name), ".postman_collection.json") {
			items = append(items, item{name, path, NodePostmanCollection})
			continue
		}

		if f.ShowNonHTTP {
			items = append(items, item{name, path, NodeOtherFile})
		}
	}

	sort.Slice(items, func(i, j int) bool {
		if items[i].kind != items[j].kind {
			return items[i].kind < items[j].kind // dirs first
		}
		return strings.ToLower(items[i].name) < strings.ToLower(items[j].name)
	})

	parent.ClearChildren()

	for _, it := range items {
		var label string
		switch it.kind {
		case NodeDir:
			label = dirLabel(iconCollapsed(), it.name)
		case NodeHTTPFile:
			label = httpFileLabel(it.path, it.name)
		case NodeBruFile:
			label = bruFileLabel(it.path, it.name)
		case NodePostmanCollection:
			label = postmanCollectionLabel(iconCollapsed(), it.name)
		default:
			label = fmt.Sprintf("[#4a4f72]%s %s[-]", iconFile(), it.name)
		}

		child := tview.NewTreeNode(label).
			SetReference(FSNode{Kind: it.kind, Path: it.path}).
			SetTextStyle(tcell.StyleDefault.Background(tcell.NewHexColor(0x0d0f1e))).
			SetSelectedTextStyle(tcell.StyleDefault.
				Background(tcell.NewHexColor(0x2d1f6e)).
				Foreground(tcell.ColorWhite))

		parent.AddChild(child)
	}
}

func isEnter(ev *tcell.EventKey) bool {
	if ev.Key() == tcell.KeyEnter {
		return true
	}
	if ev.Key() == tcell.KeyRune {
		r := ev.Rune()
		return r == '\r' || r == '\n'
	}
	return false
}


func iconExpanded() string  { return "▼" }
func iconCollapsed() string { return "▶" }
func iconFile() string      { return "·" }

// dirLabel formats a directory node label: purple arrow + light lavender name.
func dirLabel(icon, name string) string {
	return fmt.Sprintf("[#a78bfa]%s[-] [#ddd8ff]%s[#7c78a5]/[-]", icon, name)
}

// httpFileLabel reads the HTTP method from the file and returns a colored label.
func httpFileLabel(path, name string) string {
	method := peekMethod(path)
	if method == "" {
		return fmt.Sprintf("  [#b0acd0]%s[-]", name)
	}
	return httpFileLabelWithMethod(method, name)
}

// httpFileLabelWithMethod returns a label with a colored method badge.
func httpFileLabelWithMethod(method, name string) string {
	return fmt.Sprintf("  [%s]%s[-] [#b0acd0]%s[-]", methodColor(method), method, name)
}

// peekMethod reads the first request line of a .http file and returns the method.
func peekMethod(path string) string {
	f, err := os.Open(path)
	if err != nil {
		return ""
	}
	defer f.Close()
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		line := strings.TrimSpace(sc.Text())
		if line == "" || strings.HasPrefix(line, "#") || strings.HasPrefix(line, "//") {
			continue
		}
		parts := strings.Fields(line)
		if len(parts) >= 2 {
			m := strings.ToUpper(parts[0])
			switch m {
			case "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS":
				return m
			}
		}
		break
	}
	return ""
}

// bruFileLabel reads the HTTP method from the .bru file and returns a label.
func bruFileLabel(path, name string) string {
	method := peekBruMethod(path)
	if method == "" {
		return fmt.Sprintf("  [#b0acd0]%s[-]", name)
	}
	return httpFileLabelWithMethod(method, name)
}

// peekBruMethod reads the first HTTP method block keyword from a .bru file.
func peekBruMethod(path string) string {
	f, err := os.Open(path)
	if err != nil {
		return ""
	}
	defer f.Close()
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		line := strings.TrimSpace(sc.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		// Match lines like "get {", "post {", etc.
		word := strings.ToLower(strings.Fields(line)[0])
		switch word {
		case "get", "post", "put", "patch", "delete", "head", "options":
			return strings.ToUpper(word)
		}
	}
	return ""
}

// postmanCollectionLabel returns a label for a Postman collection node.
func postmanCollectionLabel(icon, name string) string {
	return fmt.Sprintf("[#f59e0b]%s[-] [#fcd34d]%s[-]", icon, name)
}

// populatePostmanCollection parses a .postman_collection.json and adds virtual child nodes.
func populatePostmanCollection(parent *tview.TreeNode, collectionPath string) {
	data, err := os.ReadFile(collectionPath)
	if err != nil {
		return
	}
	c, err := postmanfile.Parse(data)
	if err != nil {
		return
	}
	addPostmanItems(parent, collectionPath, c.Items, []int{})
}

func addPostmanItems(parent *tview.TreeNode, collectionPath string, items []postmanfile.Item, basePath []int) {
	for i, item := range items {
		itemPath := append(append([]int{}, basePath...), i)

		var label string
		if item.Request != nil {
			// It's a request leaf
			method := strings.ToUpper(item.Request.Method)
			label = httpFileLabelWithMethod(method, item.Name)
			node := tview.NewTreeNode(label).
				SetReference(PostmanNode{
					CollectionPath: collectionPath,
					ItemPath:       itemPath,
					Name:           item.Name,
					Method:         method,
				}).
				SetTextStyle(tcell.StyleDefault.Background(tcell.NewHexColor(0x0d0f1e))).
				SetSelectedTextStyle(tcell.StyleDefault.
					Background(tcell.NewHexColor(0x2d1f6e)).
					Foreground(tcell.ColorWhite))
			parent.AddChild(node)
		} else {
			// It's a folder
			label = dirLabel(iconCollapsed(), item.Name)
			node := tview.NewTreeNode(label).
				SetReference(PostmanFolderNode{Name: item.Name}).
				SetTextStyle(tcell.StyleDefault.Background(tcell.NewHexColor(0x0d0f1e))).
				SetSelectedTextStyle(tcell.StyleDefault.
					Background(tcell.NewHexColor(0x2d1f6e)).
					Foreground(tcell.ColorWhite))
			addPostmanItems(node, collectionPath, item.Items, itemPath)
			parent.AddChild(node)
		}
	}
}

// methodColor returns a hex color string for the given HTTP method.
func methodColor(m string) string {
	switch m {
	case "GET":
		return "#4ade80"
	case "POST":
		return "#fb923c"
	case "PUT":
		return "#60a5fa"
	case "PATCH":
		return "#c084fc"
	case "DELETE":
		return "#f87171"
	case "HEAD":
		return "#2dd4bf"
	default:
		return "#94a3b8"
	}
}
