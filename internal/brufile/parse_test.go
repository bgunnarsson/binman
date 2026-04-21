package brufile

import (
	"strings"
	"testing"

	"github.com/bgunnarsson/binman/internal/httpfile"
)

func TestParseBruBasic(t *testing.T) {
	src := `meta {
  name: list users
}

get {
  url: https://example.com/users
}

headers {
  Authorization: Bearer abc
  Accept: application/json
}

body:json {
  {"page": 1}
}
`
	req, err := Parse(src)
	if err != nil {
		t.Fatal(err)
	}
	if req.Method != "GET" {
		t.Errorf("method: got %q", req.Method)
	}
	if req.URL != "https://example.com/users" {
		t.Errorf("url: got %q", req.URL)
	}
	if req.Headers["Authorization"] != "Bearer abc" {
		t.Errorf("auth header missing: %v", req.Headers)
	}
	if !strings.Contains(req.Body, `"page"`) {
		t.Errorf("body missing: %q", req.Body)
	}
}

func TestFormatBruRoundTrip(t *testing.T) {
	in := &httpfile.Request{
		Method:  "POST",
		URL:     "https://api/x",
		Headers: map[string]string{"Content-Type": "application/json"},
		Body:    `{"a":1}`,
	}
	out := Format(in)
	round, err := Parse(out)
	if err != nil {
		t.Fatal(err)
	}
	if round.Method != "POST" || round.URL != in.URL {
		t.Errorf("roundtrip: %+v", round)
	}
	if !strings.Contains(round.Body, `"a"`) {
		t.Errorf("body lost: %q", round.Body)
	}
}
