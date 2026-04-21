// Package graphqlfile parses .graphql files and builds a JSON body suitable for
// POSTing to a GraphQL endpoint.
//
// A .graphql file may contain just a GraphQL document (query/mutation), or a
// document preceded by headers:
//
//	# URL: https://api.example.com/graphql
//	# Header: Authorization: Bearer {{TOKEN}}
//	---
//	query UserById($id: ID!) {
//	  user(id: $id) { name email }
//	}
//	---
//	{ "id": "42" }
//
// The first `---` separator splits metadata from the operation; the second
// separates the operation from a JSON variables block. Both separators are
// optional.
package graphqlfile

import (
	"encoding/json"
	"os"
	"strings"

	"github.com/bgunnarsson/binman/internal/httpfile"
)

// Load reads a .graphql file and returns a Request whose body is the
// application/json payload GraphQL servers expect.
func Load(path string) (*httpfile.Request, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	return Parse(string(data))
}

// Parse parses the text of a .graphql file.
func Parse(content string) (*httpfile.Request, error) {
	req := &httpfile.Request{
		Method:  "POST",
		Headers: map[string]string{"Content-Type": "application/json"},
		RawText: content,
	}

	parts := strings.Split(content, "\n---\n")
	var metaLines []string
	var query string
	var varsJSON string

	switch len(parts) {
	case 1:
		query = parts[0]
	case 2:
		metaLines = strings.Split(parts[0], "\n")
		query = parts[1]
	default:
		metaLines = strings.Split(parts[0], "\n")
		query = parts[1]
		varsJSON = strings.TrimSpace(strings.Join(parts[2:], "\n---\n"))
	}

	for _, line := range metaLines {
		line = strings.TrimSpace(line)
		line = strings.TrimPrefix(line, "#")
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		if v, ok := trimPrefixFold(line, "url:"); ok {
			req.URL = strings.TrimSpace(v)
			continue
		}
		if v, ok := trimPrefixFold(line, "header:"); ok {
			if idx := strings.Index(v, ":"); idx > 0 {
				req.Headers[strings.TrimSpace(v[:idx])] = strings.TrimSpace(v[idx+1:])
			}
		}
	}

	payload := map[string]any{"query": strings.TrimSpace(query)}
	if varsJSON != "" {
		var vars any
		if err := json.Unmarshal([]byte(varsJSON), &vars); err == nil {
			payload["variables"] = vars
		}
	}
	bodyBytes, err := json.Marshal(payload)
	if err != nil {
		return nil, err
	}
	req.Body = string(bodyBytes)
	return req, nil
}

func trimPrefixFold(s, prefix string) (string, bool) {
	if len(s) >= len(prefix) && strings.EqualFold(s[:len(prefix)], prefix) {
		return s[len(prefix):], true
	}
	return "", false
}
