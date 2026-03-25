package openapi

import (
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
// URL is composed from the first server URL in the spec (if any) plus path.
// A Content-Type: application/json header is added when the operation defines
// a JSON request body.
func OperationRequest(spec *Spec, path, method string) (*httpfile.Request, error) {
	base := ""
	if len(spec.Servers) > 0 {
		base = strings.TrimRight(spec.Servers[0].URL, "/")
	}

	req := &httpfile.Request{
		Method:  strings.ToUpper(method),
		URL:     base + path,
		Headers: map[string]string{},
	}

	item, ok := spec.Paths[path]
	if !ok {
		return req, nil
	}

	op := operationByMethod(item, method)
	if op != nil && op.RequestBody != nil {
		if _, hasJSON := op.RequestBody.Content["application/json"]; hasJSON {
			req.Headers["Content-Type"] = "application/json"
		}
	}

	return req, nil
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
