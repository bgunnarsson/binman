package brufile

import (
	"strings"

	"github.com/bgunnarsson/binman/internal/httpfile"
)

var httpMethods = map[string]bool{
	"get": true, "post": true, "put": true,
	"patch": true, "delete": true, "head": true, "options": true,
}

// block is a top-level brace block within a .bru file.
type block struct {
	name  string
	lines []string
}

// Parse parses the text content of a .bru file into an httpfile.Request.
func Parse(content string) (*httpfile.Request, error) {
	req := &httpfile.Request{
		Headers: make(map[string]string),
		RawText: content,
	}

	for _, b := range splitBlocks(content) {
		nameLower := strings.ToLower(b.name)

		if httpMethods[nameLower] {
			req.Method = strings.ToUpper(nameLower)
			for _, l := range b.lines {
				t := strings.TrimSpace(l)
				if strings.HasPrefix(t, "url:") {
					req.URL = strings.TrimSpace(strings.TrimPrefix(t, "url:"))
				}
			}
			continue
		}

		if nameLower == "headers" {
			for k, v := range parseKVLines(b.lines) {
				req.Headers[k] = v
			}
			continue
		}

		// Bruno declares request-scoped vars under `vars`, `vars:pre-request`,
		// or `vars:post-response`. Only the first two are useful for outgoing
		// resolution; post-response is set after the request runs.
		if nameLower == "vars" || nameLower == "vars:pre-request" {
			if req.Vars == nil {
				req.Vars = map[string]string{}
			}
			for k, v := range parseKVLines(b.lines) {
				req.Vars[k] = v
			}
			continue
		}

		if strings.HasPrefix(nameLower, "body:") && nameLower != "body:none" {
			req.Body = strings.TrimSpace(strings.Join(b.lines, "\n"))
			continue
		}
	}

	return req, nil
}

// ParseVarsBlocks scans .bru text for any `vars*` blocks and returns a merged
// key→value map. Used by collection.bru / folder.bru / environments/*.bru.
func ParseVarsBlocks(content string) map[string]string {
	out := map[string]string{}
	for _, b := range splitBlocks(content) {
		name := strings.ToLower(b.name)
		if name != "vars" && !strings.HasPrefix(name, "vars:") {
			continue
		}
		for k, v := range parseKVLines(b.lines) {
			out[k] = v
		}
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

// splitBlocks tokenizes a .bru file into top-level brace blocks.
func splitBlocks(content string) []block {
	lines := strings.Split(strings.ReplaceAll(content, "\r\n", "\n"), "\n")
	var blocks []block
	depth := 0
	var current *block

	for _, line := range lines {
		trimmed := strings.TrimSpace(line)

		if current == nil {
			if strings.HasSuffix(trimmed, "{") {
				name := strings.TrimSpace(strings.TrimSuffix(trimmed, "{"))
				current = &block{name: name}
				depth = 1
			}
			continue
		}

		if trimmed == "}" {
			depth--
			if depth == 0 {
				blocks = append(blocks, *current)
				current = nil
			} else {
				current.lines = append(current.lines, line)
			}
			continue
		}

		if strings.HasSuffix(trimmed, "{") {
			depth++
		}
		current.lines = append(current.lines, line)
	}
	return blocks
}

// parseKVLines reads `key: value` lines, trimming an optional leading `~` that
// Bruno uses to mark disabled entries (we honor it by skipping the line).
func parseKVLines(lines []string) map[string]string {
	out := map[string]string{}
	for _, l := range lines {
		t := strings.TrimSpace(l)
		if t == "" || strings.HasPrefix(t, "//") {
			continue
		}
		if strings.HasPrefix(t, "~") {
			continue
		}
		idx := strings.Index(t, ":")
		if idx <= 0 {
			continue
		}
		k := strings.TrimSpace(t[:idx])
		v := strings.TrimSpace(t[idx+1:])
		out[k] = v
	}
	return out
}
