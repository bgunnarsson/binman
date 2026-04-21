package envfile

import (
	"os"
	"path/filepath"
	"testing"
)

func TestParse(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, ".env")
	contents := "# comment\nFOO=bar\nBAZ = qux\n\n"
	if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
		t.Fatal(err)
	}
	v, err := Parse(path)
	if err != nil {
		t.Fatal(err)
	}
	if v["FOO"] != "bar" || v["BAZ"] != "qux" {
		t.Errorf("got %v", v)
	}
}

func TestResolve(t *testing.T) {
	got := Resolve("Hello {{NAME}}, code={{CODE}}!", map[string]string{"NAME": "world", "CODE": "42"})
	if got != "Hello world, code=42!" {
		t.Errorf("got %q", got)
	}
}

func TestResolveUnknownLeftAlone(t *testing.T) {
	got := Resolve("x={{MISSING}}", map[string]string{})
	if got != "x={{MISSING}}" {
		t.Errorf("got %q", got)
	}
}

func TestFindWalksUp(t *testing.T) {
	root := t.TempDir()
	deep := filepath.Join(root, "a", "b", "c")
	if err := os.MkdirAll(deep, 0o755); err != nil {
		t.Fatal(err)
	}
	// .env in the deep dir → label "default"
	if err := os.WriteFile(filepath.Join(deep, ".env"), []byte("X=1"), 0o644); err != nil {
		t.Fatal(err)
	}
	// .env.staging in the root → label "staging"
	if err := os.WriteFile(filepath.Join(root, ".env.staging"), []byte("Y=2"), 0o644); err != nil {
		t.Fatal(err)
	}
	files := Find(deep, root)
	if len(files) != 2 {
		t.Fatalf("want 2 files, got %d: %+v", len(files), files)
	}
	labels := map[string]bool{}
	for _, f := range files {
		labels[f.Label] = true
	}
	if !labels["default"] || !labels["staging"] {
		t.Errorf("missing labels: %v", labels)
	}
}
