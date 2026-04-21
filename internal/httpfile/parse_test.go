package httpfile

import (
	"strings"
	"testing"
)

func TestParseBasic(t *testing.T) {
	src := `POST https://example.com/api
Content-Type: application/json
X-Custom: hello

{"hi": 1}
`
	req, err := Parse(src)
	if err != nil {
		t.Fatal(err)
	}
	if req.Method != "POST" {
		t.Errorf("method: got %q want POST", req.Method)
	}
	if req.URL != "https://example.com/api" {
		t.Errorf("url: got %q", req.URL)
	}
	if req.Headers["Content-Type"] != "application/json" {
		t.Errorf("content-type header missing: %v", req.Headers)
	}
	if req.Headers["X-Custom"] != "hello" {
		t.Errorf("custom header missing")
	}
	if !strings.Contains(req.Body, `"hi"`) {
		t.Errorf("body missing: %q", req.Body)
	}
}

func TestParseLowercaseMethod(t *testing.T) {
	req, _ := Parse("get https://x")
	if req.Method != "GET" {
		t.Errorf("method should uppercase: got %q", req.Method)
	}
}

func TestParseEmpty(t *testing.T) {
	req, err := Parse("")
	if err != nil {
		t.Fatal(err)
	}
	if req.Method != "" || req.URL != "" {
		t.Errorf("empty input should produce empty request: %+v", req)
	}
}

func TestFormatRoundTrip(t *testing.T) {
	in := &Request{
		Method:  "PUT",
		URL:     "https://api.example.com/x",
		Headers: map[string]string{"Content-Type": "application/json"},
		Body:    `{"k":1}`,
	}
	out := Format(in)
	round, err := Parse(out)
	if err != nil {
		t.Fatal(err)
	}
	if round.Method != in.Method || round.URL != in.URL {
		t.Errorf("roundtrip: %+v", round)
	}
	if round.Headers["Content-Type"] != "application/json" {
		t.Errorf("header lost in roundtrip: %v", round.Headers)
	}
	if strings.TrimSpace(round.Body) != strings.TrimSpace(in.Body) {
		t.Errorf("body lost in roundtrip: got %q want %q", round.Body, in.Body)
	}
}
