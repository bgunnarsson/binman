package brufile

import (
	"os"
	"path/filepath"
	"testing"
)

func TestParseRequestLevelVars(t *testing.T) {
	src := `get {
  url: https://example.com
}

vars {
  userId: 42
}

vars:pre-request {
  trace: abc
}
`
	req, err := Parse(src)
	if err != nil {
		t.Fatal(err)
	}
	if req.Vars["userId"] != "42" {
		t.Errorf("userId: %v", req.Vars)
	}
	if req.Vars["trace"] != "abc" {
		t.Errorf("trace: %v", req.Vars)
	}
}

func TestParseVarsBlocksMerge(t *testing.T) {
	src := `vars {
  a: 1
  b: 2
}

vars:secret {
  token: xyz
}
`
	got := ParseVarsBlocks(src)
	if got["a"] != "1" || got["b"] != "2" || got["token"] != "xyz" {
		t.Errorf("got %v", got)
	}
}

func TestCollectionVarsWalksUp(t *testing.T) {
	root := t.TempDir()
	collDir := filepath.Join(root, "coll")
	folderDir := filepath.Join(collDir, "users")
	if err := os.MkdirAll(folderDir, 0o755); err != nil {
		t.Fatal(err)
	}

	if err := os.WriteFile(filepath.Join(collDir, "collection.bru"),
		[]byte("vars {\n  base: https://api.example.com\n  shared: c\n}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(folderDir, "folder.bru"),
		[]byte("vars {\n  shared: f\n}\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	got := CollectionVars(folderDir, root)
	if got["base"] != "https://api.example.com" {
		t.Errorf("missing base: %v", got)
	}
	if got["shared"] != "f" {
		t.Errorf("folder.bru should override: %v", got)
	}
}

func TestFindEnvironments(t *testing.T) {
	root := t.TempDir()
	collDir := filepath.Join(root, "coll")
	envDir := filepath.Join(collDir, "environments")
	deep := filepath.Join(collDir, "auth")
	if err := os.MkdirAll(envDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(deep, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(envDir, "dev.bru"),
		[]byte("vars {\n  host: dev.example.com\n}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(envDir, "prod.bru"),
		[]byte("vars {\n  host: prod.example.com\n}\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	envs := FindEnvironments(deep, root)
	if len(envs) != 2 {
		t.Fatalf("want 2 envs, got %d: %+v", len(envs), envs)
	}
	labels := map[string]bool{}
	for _, e := range envs {
		labels[e.Label] = true
	}
	if !labels["dev"] || !labels["prod"] {
		t.Errorf("missing labels: %v", labels)
	}

	vars, err := LoadEnvironment(envs[0].Path)
	if err != nil {
		t.Fatal(err)
	}
	if vars["host"] == "" {
		t.Errorf("vars empty: %v", vars)
	}
}
