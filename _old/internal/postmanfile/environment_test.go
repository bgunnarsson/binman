package postmanfile

import (
	"os"
	"path/filepath"
	"testing"
)

const sampleEnvironment = `{
  "name": "Dev",
  "values": [
    {"key": "BASE", "value": "https://api.dev.example.com", "enabled": true},
    {"key": "TOKEN", "value": "abc"},
    {"key": "DISABLED", "value": "skip", "enabled": false}
  ]
}`

func TestParseEnvironment(t *testing.T) {
	got, err := ParseEnvironment([]byte(sampleEnvironment))
	if err != nil {
		t.Fatal(err)
	}
	if got["BASE"] != "https://api.dev.example.com" {
		t.Errorf("BASE: %v", got)
	}
	if got["TOKEN"] != "abc" {
		t.Errorf("TOKEN (enabled defaults true): %v", got)
	}
	if _, ok := got["DISABLED"]; ok {
		t.Errorf("disabled entries should be skipped: %v", got)
	}
}

func TestFindEnvironmentsWalksUp(t *testing.T) {
	root := t.TempDir()
	deep := filepath.Join(root, "api", "v1")
	if err := os.MkdirAll(deep, 0o755); err != nil {
		t.Fatal(err)
	}

	if err := os.WriteFile(filepath.Join(root, "dev.postman_environment.json"),
		[]byte(sampleEnvironment), 0o644); err != nil {
		t.Fatal(err)
	}
	prodPath := filepath.Join(deep, "prod.postman_environment.json")
	if err := os.WriteFile(prodPath, []byte(sampleEnvironment), 0o644); err != nil {
		t.Fatal(err)
	}

	envs := FindEnvironments(deep, root)
	if len(envs) != 2 {
		t.Fatalf("want 2, got %d: %+v", len(envs), envs)
	}
	labels := map[string]bool{}
	for _, e := range envs {
		labels[e.Label] = true
	}
	if !labels["dev"] || !labels["prod"] {
		t.Errorf("missing labels: %v", labels)
	}
}
