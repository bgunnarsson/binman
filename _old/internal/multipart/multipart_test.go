package multipart

import (
	"bytes"
	"mime"
	"mime/multipart"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestEncodeFields(t *testing.T) {
	body, ct, err := Encode([]Field{
		{Name: "name", Value: "alice"},
		{Name: "age", Value: "30"},
	})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(ct, "multipart/form-data") {
		t.Errorf("ct: %q", ct)
	}
	mt, params, _ := mime.ParseMediaType(ct)
	if mt != "multipart/form-data" {
		t.Fatalf("media type: %q", mt)
	}
	mr := multipart.NewReader(bytes.NewReader(body), params["boundary"])
	form, err := mr.ReadForm(1 << 20)
	if err != nil {
		t.Fatal(err)
	}
	if form.Value["name"][0] != "alice" || form.Value["age"][0] != "30" {
		t.Errorf("form: %+v", form.Value)
	}
}

func TestEncodeFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "data.txt")
	if err := os.WriteFile(path, []byte("hello world"), 0o644); err != nil {
		t.Fatal(err)
	}
	body, ct, err := Encode([]Field{{Name: "upload", Value: "@" + path}})
	if err != nil {
		t.Fatal(err)
	}
	_, params, _ := mime.ParseMediaType(ct)
	mr := multipart.NewReader(bytes.NewReader(body), params["boundary"])
	form, err := mr.ReadForm(1 << 20)
	if err != nil {
		t.Fatal(err)
	}
	files := form.File["upload"]
	if len(files) == 0 {
		t.Fatal("no file part")
	}
	if files[0].Filename != "data.txt" {
		t.Errorf("filename: %q", files[0].Filename)
	}
}
