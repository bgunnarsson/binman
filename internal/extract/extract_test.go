package extract

import (
	"net/http"
	"testing"
)

func TestParseRules(t *testing.T) {
	src := `# header comment
TOKEN = json data.access_token
ID = regex /id=(\d+)/
SERVER = header Server
malformed line
`
	rules := ParseRules(src)
	if len(rules) != 3 {
		t.Fatalf("got %d rules: %+v", len(rules), rules)
	}
	if rules[0].Name != "TOKEN" || rules[0].Kind != "json" || rules[0].Arg != "data.access_token" {
		t.Errorf("rule 0: %+v", rules[0])
	}
}

func TestApplyJSON(t *testing.T) {
	rules := []Rule{
		{Name: "ID", Kind: "json", Arg: "data.0.id"},
		{Name: "NAME", Kind: "json", Arg: "data.0.name"},
		{Name: "ACTIVE", Kind: "json", Arg: "data.0.active"},
		{Name: "MISSING", Kind: "json", Arg: "data.99.id"},
	}
	body := `{"data":[{"id": 42, "name": "alice", "active": true}]}`
	out := Apply(rules, body, nil)
	if out["ID"] != "42" {
		t.Errorf("ID: %q", out["ID"])
	}
	if out["NAME"] != "alice" {
		t.Errorf("NAME: %q", out["NAME"])
	}
	if out["ACTIVE"] != "true" {
		t.Errorf("ACTIVE: %q", out["ACTIVE"])
	}
	if _, ok := out["MISSING"]; ok {
		t.Errorf("MISSING should be absent")
	}
}

func TestApplyRegex(t *testing.T) {
	rules := []Rule{{Name: "ID", Kind: "regex", Arg: `/id=(\d+)/`}}
	out := Apply(rules, "user id=4711 something", nil)
	if out["ID"] != "4711" {
		t.Errorf("got %q", out["ID"])
	}
}

func TestApplyHeader(t *testing.T) {
	h := http.Header{}
	h.Set("X-Trace-Id", "abc123")
	out := Apply([]Rule{{Name: "TID", Kind: "header", Arg: "X-Trace-Id"}}, "", h)
	if out["TID"] != "abc123" {
		t.Errorf("got %v", out)
	}
}
