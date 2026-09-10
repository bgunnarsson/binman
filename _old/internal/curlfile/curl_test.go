package curlfile

import (
	"strings"
	"testing"
)

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

func TestParseDataImpliesFormContentType(t *testing.T) {
	req, _ := Parse(`curl -d "foo=bar" https://x`)
	if req.Headers["Content-Type"] != "application/x-www-form-urlencoded" {
		t.Errorf("expected implicit form content type, got %v", req.Headers)
	}
}

func TestParseOAuth2ClientCredentials(t *testing.T) {
	// The OAuth2 client_credentials token request: auth lives entirely in the
	// form body. curl sends it as application/x-www-form-urlencoded, and the
	// client_secret comes via --data-urlencode.
	src := `curl -s -X POST 'https://login.microsoftonline.com/tid/oauth2/v2.0/token' ` +
		`-d 'grant_type=client_credentials' ` +
		`-d 'client_id=abc' ` +
		`-d 'scope=https://graph.microsoft.com/.default' ` +
		`--data-urlencode 'client_secret=sec.ret~with/chars'`
	req, err := Parse(src)
	if err != nil {
		t.Fatal(err)
	}
	if req.Method != "POST" {
		t.Errorf("method: %q", req.Method)
	}
	if req.Headers["Content-Type"] != "application/x-www-form-urlencoded" {
		t.Errorf("content-type: %v", req.Headers)
	}
	// All four form fields must survive, including the url-encoded secret.
	for _, want := range []string{
		"grant_type=client_credentials",
		"client_id=abc",
		"scope=https://graph.microsoft.com/.default", // -d passes through raw
		"client_secret=sec.ret~with%2Fchars",         // --data-urlencode encodes
	} {
		if !strings.Contains(req.Body, want) {
			t.Errorf("body missing %q; got %q", want, req.Body)
		}
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
