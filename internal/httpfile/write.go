package httpfile

import (
	"fmt"
	"os"
	"sort"
	"strings"
)

// Format renders req back into the plain .http wire format:
//
//	METHOD URL
//	Header-Name: value
//	(blank line)
//	body...
func Format(req *Request) string {
	var b strings.Builder
	method := req.Method
	if method == "" {
		method = "GET"
	}
	fmt.Fprintf(&b, "%s %s\n", method, req.URL)

	keys := make([]string, 0, len(req.Headers))
	for k := range req.Headers {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		fmt.Fprintf(&b, "%s: %s\n", k, req.Headers[k])
	}
	if req.Body != "" {
		b.WriteString("\n")
		b.WriteString(req.Body)
		if !strings.HasSuffix(req.Body, "\n") {
			b.WriteString("\n")
		}
	}
	return b.String()
}

// Save writes req to path in .http format, overwriting any existing file.
func Save(path string, req *Request) error {
	return os.WriteFile(path, []byte(Format(req)), 0o644)
}
