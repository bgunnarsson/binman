package history

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestAppendAndLoad(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_STATE_HOME", dir)

	for i := 0; i < 3; i++ {
		Append(Entry{
			Timestamp: time.Now().Add(time.Duration(i) * time.Second),
			Method:    "GET",
			URL:       "https://example.com",
			Status:    200,
			Duration:  50 * time.Millisecond,
		})
	}

	entries, err := Load(0)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 3 {
		t.Errorf("got %d entries", len(entries))
	}

	// Verify file path is XDG_STATE_HOME-rooted
	wantPath := filepath.Join(dir, "binman", "history.jsonl")
	if _, err := os.Stat(wantPath); err != nil {
		t.Errorf("expected file at %s: %v", wantPath, err)
	}
}

func TestLoadEmpty(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_STATE_HOME", dir)
	entries, err := Load(0)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 0 {
		t.Errorf("expected empty, got %+v", entries)
	}
}
