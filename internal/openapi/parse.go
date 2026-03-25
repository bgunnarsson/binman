package openapi

import (
	"bytes"
	"encoding/json"
	"fmt"
	"strings"

	"gopkg.in/yaml.v3"
)

// Parse decodes an OpenAPI spec from data. filename is used to choose JSON vs YAML.
func Parse(data []byte, filename string) (*Spec, error) {
	var spec Spec
	var err error
	if strings.HasSuffix(strings.ToLower(filename), ".json") {
		err = json.Unmarshal(data, &spec)
	} else {
		err = yaml.Unmarshal(data, &spec)
	}
	if err != nil {
		return nil, fmt.Errorf("openapi: parse %s: %w", filename, err)
	}
	return &spec, nil
}

// IsOpenAPISpec reports whether data looks like an OpenAPI/Swagger document.
// Only the first 1 KB is inspected so callers can pass a short peek buffer.
func IsOpenAPISpec(data []byte, filename string) bool {
	peek := data
	if len(peek) > 1024 {
		peek = peek[:1024]
	}
	lower := strings.ToLower(filename)
	if strings.HasSuffix(lower, ".json") {
		return bytes.Contains(peek, []byte(`"openapi"`)) ||
			bytes.Contains(peek, []byte(`"swagger"`))
	}
	// YAML: look for a top-level key on its own line
	for _, line := range bytes.Split(peek, []byte("\n")) {
		trimmed := bytes.TrimSpace(line)
		if bytes.HasPrefix(trimmed, []byte("openapi:")) ||
			bytes.HasPrefix(trimmed, []byte("swagger:")) {
			return true
		}
	}
	return false
}
