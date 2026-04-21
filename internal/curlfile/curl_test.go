package curlfile

import "testing"

func TestIsCurl(t *testing.T) {
	cases := map[string]bool{
		"curl https://x":      true,
		"  curl https://x":    true, // leading whitespace tolerated
		"curl https://x ":     true,
		"curl":                true,
		"$ curl https://x":    true,
		"wget https://x":      false,
		"https://example.com": false,
	}
	for in, want := range cases {
		if got := IsCurl(in); got != want {
			t.Errorf("IsCurl(%q)=%v want %v", in, got, want)
		}
	}
}

func TestParseSimpleGet(t *testing.T) {
	req, err := Parse(`curl https://example.com/x`)
	if err != nil {
		t.Fatal(err)
	}
	if req.Method != "GET" || req.URL != "https://example.com/x" {
		t.Errorf("got %+v", req)
	}
}

func TestParsePostWithHeadersAndData(t *testing.T) {
	req, err := Parse(`curl -X POST -H "Content-Type: application/json" -H 'Authorization: Bearer abc' --data-raw '{"a":1}' https://example.com/api`)
	if err != nil {
		t.Fatal(err)
	}
	if req.Method != "POST" {
		t.Errorf("method: %q", req.Method)
	}
	if req.URL != "https://example.com/api" {
		t.Errorf("url: %q", req.URL)
	}
	if req.Headers["Content-Type"] != "application/json" {
		t.Errorf("ct: %v", req.Headers)
	}
	if req.Headers["Authorization"] != "Bearer abc" {
		t.Errorf("auth: %v", req.Headers)
	}
	if req.Body != `{"a":1}` {
		t.Errorf("body: %q", req.Body)
	}
}

func TestParseDataImpliesPost(t *testing.T) {
	req, _ := Parse(`curl -d "foo=bar" https://x`)
	if req.Method != "POST" {
		t.Errorf("expected POST, got %q", req.Method)
	}
}

func TestParseUserBasicAuth(t *testing.T) {
	req, _ := Parse(`curl -u alice:secret https://x`)
	if req.Headers["Authorization"] != "Basic YWxpY2U6c2VjcmV0" {
		t.Errorf("auth: %v", req.Headers)
	}
}

func TestParseLineContinuation(t *testing.T) {
	src := "curl -X POST \\\n  -H 'A: 1' \\\n  https://x"
	req, err := Parse(src)
	if err != nil {
		t.Fatal(err)
	}
	if req.URL != "https://x" || req.Headers["A"] != "1" {
		t.Errorf("got %+v", req)
	}
}
