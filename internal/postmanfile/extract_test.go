package postmanfile

import (
	"strings"
	"testing"
)

const sampleCollection = `{
  "info": {"name": "demo"},
  "variable": [{"key": "BASE", "value": "https://api.example.com"}],
  "item": [
    {
      "name": "Folder",
      "item": [
        {
          "name": "List users",
          "request": {
            "method": "GET",
            "url": "{{BASE}}/users",
            "header": [{"key": "Accept", "value": "application/json"}]
          }
        }
      ]
    }
  ]
}`

func TestParseAndRequestAt(t *testing.T) {
	c, err := Parse([]byte(sampleCollection))
	if err != nil {
		t.Fatal(err)
	}
	if c.Vars()["BASE"] != "https://api.example.com" {
		t.Errorf("vars: %+v", c.Vars())
	}
	req, err := RequestAt(c, []int{0, 0})
	if err != nil {
		t.Fatal(err)
	}
	if req.Method != "GET" {
		t.Errorf("method: %q", req.Method)
	}
	if !strings.Contains(req.URL, "{{BASE}}") {
		t.Errorf("url: %q", req.URL)
	}
	if req.Headers["Accept"] != "application/json" {
		t.Errorf("headers: %v", req.Headers)
	}
}

func TestRequestAtFolderError(t *testing.T) {
	c, _ := Parse([]byte(sampleCollection))
	if _, err := RequestAt(c, []int{0}); err == nil {
		t.Errorf("expected error for folder path")
	}
}
