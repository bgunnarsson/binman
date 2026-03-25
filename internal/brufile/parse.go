package brufile

import (
	"strings"

	"github.com/bgunnarsson/binman/internal/httpfile"
)

var httpMethods = map[string]bool{
	"get": true, "post": true, "put": true,
	"patch": true, "delete": true, "head": true, "options": true,
}

// Parse parses the text content of a .bru file into an httpfile.Request.
func Parse(content string) (*httpfile.Request, error) {
	req := &httpfile.Request{
		Headers: make(map[string]string),
		RawText: content,
	}

	lines := strings.Split(strings.ReplaceAll(content, "\r\n", "\n"), "\n")

	type block struct {
		name  string
		lines []string
	}

	// Parse top-level blocks
	var blocks []block
	depth := 0
	var current *block

	for _, line := range lines {
		trimmed := strings.TrimSpace(line)

		if current == nil {
			// Look for a block opener: "word {" or "word:suffix {"
			if strings.HasSuffix(trimmed, "{") {
				name := strings.TrimSuffix(trimmed, "{")
				name = strings.TrimSpace(name)
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

	for _, b := range blocks {
		nameLower := strings.ToLower(b.name)

		// HTTP method block: "get", "post", etc.
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

		// headers block
		if nameLower == "headers" {
			for _, l := range b.lines {
				t := strings.TrimSpace(l)
				if t == "" {
					continue
				}
				idx := strings.Index(t, ":")
				if idx > 0 {
					k := strings.TrimSpace(t[:idx])
					v := strings.TrimSpace(t[idx+1:])
					req.Headers[k] = v
				}
			}
			continue
		}

		// body:* blocks
		if strings.HasPrefix(nameLower, "body:") && nameLower != "body:none" {
			req.Body = strings.TrimSpace(strings.Join(b.lines, "\n"))
			continue
		}
	}

	return req, nil
}
