package httpfile

import (
	"strings"
)

// Parse parses the text content of a .http file into a Request.
func Parse(content string) (*Request, error) {
	req := &Request{
		Headers: make(map[string]string),
		RawText: content,
	}

	lines := strings.Split(strings.ReplaceAll(content, "\r\n", "\n"), "\n")

	if len(lines) == 0 {
		return req, nil
	}

	// First non-empty line: METHOD URL
	firstLine := ""
	startIdx := 0
	for i, l := range lines {
		if strings.TrimSpace(l) != "" {
			firstLine = strings.TrimSpace(l)
			startIdx = i + 1
			break
		}
	}

	if firstLine != "" {
		parts := strings.SplitN(firstLine, " ", 2)
		if len(parts) >= 1 {
			req.Method = strings.ToUpper(parts[0])
		}
		if len(parts) >= 2 {
			req.URL = strings.TrimSpace(parts[1])
		}
	}

	// Headers until blank line
	i := startIdx
	for i < len(lines) {
		line := lines[i]
		if strings.TrimSpace(line) == "" {
			i++
			break
		}
		idx := strings.Index(line, ":")
		if idx > 0 {
			key := strings.TrimSpace(line[:idx])
			val := strings.TrimSpace(line[idx+1:])
			req.Headers[key] = val
		}
		i++
	}

	// Body: rest of lines
	if i < len(lines) {
		req.Body = strings.TrimSpace(strings.Join(lines[i:], "\n"))
	}

	return req, nil
}
