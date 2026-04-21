package fsview

import (
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/bgunnarsson/binman/internal/openapi"
	"github.com/bgunnarsson/binman/internal/postmanfile"
)

// SearchEntry is one searchable request across all loaded collections.
type SearchEntry struct {
	Label    string // human-readable, e.g. "GET users.http"
	Kind     NodeKind
	Path     string
	ItemPath []int  // postman-only
	OpPath   string // openapi-only
	Method   string // openapi-only
}

// IndexAll walks the collections root and returns every callable request.
// Postman/OpenAPI files are parsed so individual operations show up.
func IndexAll(root string) []SearchEntry {
	var out []SearchEntry
	_ = filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			if d != nil && d.IsDir() && strings.HasPrefix(d.Name(), ".") && path != root {
				return filepath.SkipDir
			}
			return nil
		}
		if strings.HasPrefix(d.Name(), ".") {
			return nil
		}
		ext := strings.ToLower(filepath.Ext(d.Name()))
		switch {
		case ext == ".http", ext == ".bru", ext == ".graphql":
			out = append(out, SearchEntry{
				Label: ext[1:] + "  " + relPath(root, path),
				Kind:  NodeHTTPFile,
				Path:  path,
			})
		case strings.HasSuffix(strings.ToLower(d.Name()), ".postman_collection.json"):
			data, err := os.ReadFile(path)
			if err != nil {
				return nil
			}
			c, err := postmanfile.Parse(data)
			if err != nil {
				return nil
			}
			collectPostman(c.Items, path, []int{}, &out, relPath(root, path))
		case ext == ".yaml", ext == ".yml", ext == ".json":
			data, err := os.ReadFile(path)
			if err != nil {
				return nil
			}
			peek := data
			if len(peek) > 2048 {
				peek = peek[:2048]
			}
			if !openapi.IsOpenAPISpec(peek, d.Name()) {
				return nil
			}
			spec, err := openapi.Parse(data, d.Name())
			if err != nil {
				return nil
			}
			rel := relPath(root, path)
			for _, g := range openapi.GroupedOperations(spec) {
				for _, op := range g.Operations {
					out = append(out, SearchEntry{
						Label:  op.Method + "  " + rel + "  " + op.Path,
						Kind:   NodeOpenAPIOperation,
						Path:   path,
						OpPath: op.Path,
						Method: op.Method,
					})
				}
			}
		}
		return nil
	})

	sort.Slice(out, func(i, j int) bool { return out[i].Label < out[j].Label })
	return out
}

func collectPostman(items []postmanfile.Item, collectionPath string, base []int, out *[]SearchEntry, rel string) {
	for i, it := range items {
		path := append(append([]int{}, base...), i)
		if it.Request != nil {
			label := strings.ToUpper(it.Request.Method) + "  " + rel + "  " + it.Name
			*out = append(*out, SearchEntry{
				Label:    label,
				Kind:     NodePostmanRequest,
				Path:     collectionPath,
				ItemPath: path,
			})
			continue
		}
		collectPostman(it.Items, collectionPath, path, out, rel)
	}
}

// FuzzyMatch returns entries whose label matches the query in order, with a
// simple subsequence match. Empty query returns the original list (truncated).
func FuzzyMatch(entries []SearchEntry, query string) []SearchEntry {
	if query == "" {
		if len(entries) > 200 {
			return entries[:200]
		}
		return entries
	}
	q := strings.ToLower(query)
	var out []SearchEntry
	for _, e := range entries {
		if subseq(strings.ToLower(e.Label), q) {
			out = append(out, e)
		}
	}
	return out
}

func subseq(haystack, needle string) bool {
	i := 0
	for j := 0; j < len(haystack) && i < len(needle); j++ {
		if haystack[j] == needle[i] {
			i++
		}
	}
	return i == len(needle)
}

func relPath(root, path string) string {
	rel, err := filepath.Rel(root, path)
	if err != nil {
		return path
	}
	return rel
}
