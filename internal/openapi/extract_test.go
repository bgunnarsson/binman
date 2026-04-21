package openapi

import (
	"strings"
	"testing"
)

const sampleSpec = `{
  "openapi": "3.0.0",
  "info": {"title": "demo"},
  "servers": [{"url": "https://api.example.com/v1"}],
  "paths": {
    "/users/{id}": {
      "get": {
        "summary": "Get user",
        "tags": ["users"],
        "parameters": [
          {"name": "verbose", "in": "query"},
          {"name": "X-Trace", "in": "header"}
        ]
      }
    }
  }
}`

func TestOperationRequest(t *testing.T) {
	spec, err := Parse([]byte(sampleSpec), "demo.json")
	if err != nil {
		t.Fatal(err)
	}
	req, err := OperationRequest(spec, "/users/{id}", "GET")
	if err != nil {
		t.Fatal(err)
	}
	if req.Method != "GET" {
		t.Errorf("method: %q", req.Method)
	}
	if !strings.Contains(req.URL, "{{id}}") {
		t.Errorf("path templating broken: %q", req.URL)
	}
	if !strings.Contains(req.URL, "verbose") {
		t.Errorf("query param missing: %q", req.URL)
	}
	if _, ok := req.Headers["X-Trace"]; !ok {
		t.Errorf("header param missing: %v", req.Headers)
	}
}

func TestGroupedOperations(t *testing.T) {
	spec, _ := Parse([]byte(sampleSpec), "demo.json")
	groups := GroupedOperations(spec)
	if len(groups) != 1 || groups[0].Tag != "users" {
		t.Errorf("groups: %+v", groups)
	}
}

func TestIsOpenAPISpec(t *testing.T) {
	if !IsOpenAPISpec([]byte(`{"openapi": "3.0.0"}`), "x.json") {
		t.Error("should detect openapi json")
	}
	if !IsOpenAPISpec([]byte("openapi: 3.0.0\ninfo:"), "x.yaml") {
		t.Error("should detect openapi yaml")
	}
	if IsOpenAPISpec([]byte(`{"foo": 1}`), "x.json") {
		t.Error("should not detect arbitrary json")
	}
}
