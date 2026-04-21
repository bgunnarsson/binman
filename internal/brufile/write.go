package brufile

import (
	"fmt"
	"os"
	"sort"
	"strings"

	"github.com/bgunnarsson/binman/internal/httpfile"
)

// Format renders req back into the .bru block format.
func Format(req *httpfile.Request) string {
	var b strings.Builder
	method := strings.ToLower(req.Method)
	if method == "" {
		method = "get"
	}
	fmt.Fprintf(&b, "%s {\n  url: %s\n}\n", method, req.URL)

	if len(req.Headers) > 0 {
		b.WriteString("\nheaders {\n")
		keys := make([]string, 0, len(req.Headers))
		for k := range req.Headers {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		for _, k := range keys {
			fmt.Fprintf(&b, "  %s: %s\n", k, req.Headers[k])
		}
		b.WriteString("}\n")
	}

	if req.Body != "" {
		bodyType := bodyBlockName(req.Headers["Content-Type"])
		fmt.Fprintf(&b, "\n%s {\n%s\n}\n", bodyType, req.Body)
	}
	return b.String()
}

// Save writes req to path in .bru format.
func Save(path string, req *httpfile.Request) error {
	return os.WriteFile(path, []byte(Format(req)), 0o644)
}

func bodyBlockName(contentType string) string {
	ct := strings.ToLower(strings.SplitN(contentType, ";", 2)[0])
	switch strings.TrimSpace(ct) {
	case "application/json":
		return "body:json"
	case "application/xml", "text/xml":
		return "body:xml"
	case "text/plain":
		return "body:text"
	case "application/x-www-form-urlencoded":
		return "body:form-urlencoded"
	case "multipart/form-data":
		return "body:multipart-form"
	}
	return "body:text"
}
