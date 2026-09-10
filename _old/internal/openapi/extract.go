package openapi

import (
	"net/url"
	"sort"
	"strings"

	"github.com/bgunnarsson/binman/internal/httpfile"
)

// TagGroup is a set of operations sharing a tag (or "Default").
type TagGroup struct {
	Tag        string
	Operations []OperationEntry
}

// OperationEntry identifies a single path+method pair.
type OperationEntry struct {
	Path    string
	Method  string
	Summary string
}

// GroupedOperations returns all operations from spec grouped by their first tag.
// Operations with no tags are placed in a "Default" group.
// Groups and operations within each group are sorted alphabetically.
func GroupedOperations(spec *Spec) []TagGroup {
	type key struct{ tag, path, method string }
	grouped := map[string][]OperationEntry{}

	orderedMethods := []struct {
		name string
		op   func(PathItem) *Operation
	}{
		{"GET", func(p PathItem) *Operation { return p.Get }},
		{"POST", func(p PathItem) *Operation { return p.Post }},
		{"PUT", func(p PathItem) *Operation { return p.Put }},
		{"PATCH", func(p PathItem) *Operation { return p.Patch }},
		{"DELETE", func(p PathItem) *Operation { return p.Delete }},
		{"HEAD", func(p PathItem) *Operation { return p.Head }},
		{"OPTIONS", func(p PathItem) *Operation { return p.Options }},
	}

	// Collect sorted paths for deterministic output
	paths := make([]string, 0, len(spec.Paths))
	for p := range spec.Paths {
		paths = append(paths, p)
	}
	sort.Strings(paths)

	for _, path := range paths {
		item := spec.Paths[path]
		for _, m := range orderedMethods {
			op := m.op(item)
			if op == nil {
				continue
			}
			tag := "Default"
			if len(op.Tags) > 0 {
				tag = op.Tags[0]
			}
			grouped[tag] = append(grouped[tag], OperationEntry{
				Path:    path,
				Method:  m.name,
				Summary: op.Summary,
			})
		}
	}

	tags := make([]string, 0, len(grouped))
	for t := range grouped {
		tags = append(tags, t)
	}
	sort.Strings(tags)

	result := make([]TagGroup, 0, len(tags))
	for _, t := range tags {
		result = append(result, TagGroup{Tag: t, Operations: grouped[t]})
	}
	return result
}

// OperationRequest builds an httpfile.Request for the given path and method.
//
// URL base: uses the first server URL in the spec. If absent or relative,
// falls back to {{URL}} so the user can supply it via .env.
//
// Parameters are resolved as follows:
//   - path params  ({param}) → converted to {{param}} in the URL for env-var resolution
//   - query params → appended to the URL query string (shown in the Params tab)
//   - header params → added to req.Headers (shown in the Headers tab)
//
// Path-level parameters are merged with operation-level parameters;
// operation-level parameters take precedence on duplicate names.
//
// A Content-Type: application/json header is added when the operation defines
// a JSON request body.
func OperationRequest(spec *Spec, path, method string) (*httpfile.Request, error) {
	base := ""
	if len(spec.Servers) > 0 {
		base = strings.TrimRight(spec.Servers[0].URL, "/")
	}
	if !strings.Contains(base, "://") {
		base = "{{URL}}" + base
	}

	// Convert OpenAPI path params {param} → binman template {{param}}.
	resolvedPath := pathToTemplate(path)

	req := &httpfile.Request{
		Method:  strings.ToUpper(method),
		URL:     base + resolvedPath,
		Headers: map[string]string{},
	}

	item, ok := spec.Paths[path]
	if !ok {
		return req, nil
	}

	op := operationByMethod(item, method)

	// Merge path-level and operation-level parameters; operation wins on conflict.
	params := mergeParams(item.Parameters, nil)
	if op != nil {
		params = mergeParams(item.Parameters, op.Parameters)
	}

	queryVals := url.Values{}
	for _, p := range params {
		switch strings.ToLower(p.In) {
		case "query":
			queryVals.Set(p.Name, "")
		case "header":
			req.Headers[p.Name] = ""
		// "path" params are already handled by pathToTemplate above.
		// "cookie" params are not supported in the UI.
		}
	}
	if len(queryVals) > 0 {
		req.URL += "?" + queryVals.Encode()
	}

	if op != nil && op.RequestBody != nil {
		if _, hasJSON := op.RequestBody.Content["application/json"]; hasJSON {
			req.Headers["Content-Type"] = "application/json"
		}
	}

	return req, nil
}

// pathToTemplate converts OpenAPI single-brace path params ({param}) to the
// binman double-brace template syntax ({{param}}).
func pathToTemplate(path string) string {
	var b strings.Builder
	for i := 0; i < len(path); i++ {
		if path[i] == '{' {
			end := strings.IndexByte(path[i:], '}')
			if end > 0 {
				name := path[i+1 : i+end]
				b.WriteString("{{")
				b.WriteString(name)
				b.WriteString("}}")
				i += end
				continue
			}
		}
		b.WriteByte(path[i])
	}
	return b.String()
}

// mergeParams merges path-level and operation-level parameters.
// Operation-level entries take precedence when the same name+in appears in both.
func mergeParams(pathParams, opParams []Parameter) []Parameter {
	type key struct{ name, in string }
	seen := map[key]bool{}
	var result []Parameter
	for _, p := range opParams {
		seen[key{p.Name, p.In}] = true
		result = append(result, p)
	}
	for _, p := range pathParams {
		if !seen[key{p.Name, p.In}] {
			result = append(result, p)
		}
	}
	return result
}

func operationByMethod(item PathItem, method string) *Operation {
	switch strings.ToUpper(method) {
	case "GET":
		return item.Get
	case "POST":
		return item.Post
	case "PUT":
		return item.Put
	case "PATCH":
		return item.Patch
	case "DELETE":
		return item.Delete
	case "HEAD":
		return item.Head
	case "OPTIONS":
		return item.Options
	}
	return nil
}
