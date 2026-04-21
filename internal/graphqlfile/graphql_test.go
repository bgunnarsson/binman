package graphqlfile

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestParseQueryOnly(t *testing.T) {
	req, err := Parse(`query { me { id } }`)
	if err != nil {
		t.Fatal(err)
	}
	if req.Method != "POST" {
		t.Errorf("method: %q", req.Method)
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(req.Body), &payload); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(payload["query"].(string), "me") {
		t.Errorf("body: %v", payload)
	}
}

func TestParseWithMetaAndVariables(t *testing.T) {
	src := `# URL: https://api.example.com/graphql
# Header: Authorization: Bearer abc
---
query UserById($id: ID!) {
  user(id: $id) { name }
}
---
{"id": "42"}
`
	req, err := Parse(src)
	if err != nil {
		t.Fatal(err)
	}
	if req.URL != "https://api.example.com/graphql" {
		t.Errorf("url: %q", req.URL)
	}
	if req.Headers["Authorization"] != "Bearer abc" {
		t.Errorf("auth: %v", req.Headers)
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(req.Body), &payload); err != nil {
		t.Fatal(err)
	}
	if payload["variables"].(map[string]any)["id"] != "42" {
		t.Errorf("variables: %v", payload["variables"])
	}
}
