// Package multipart serializes form fields and file attachments into a
// multipart/form-data body. Fields whose value starts with "@" are treated as
// file paths and uploaded as the file's contents.
package multipart

import (
	"bytes"
	"fmt"
	"io"
	"mime/multipart"
	"os"
	"path/filepath"
	"strings"
)

// Field is one form entry. If Value starts with "@", the rest is interpreted
// as a path and the file's contents are uploaded as the field value.
type Field struct {
	Name  string
	Value string
}

// Encode returns the encoded body bytes and the matching Content-Type
// (which includes the boundary).
func Encode(fields []Field) ([]byte, string, error) {
	var buf bytes.Buffer
	w := multipart.NewWriter(&buf)
	for _, f := range fields {
		if f.Name == "" {
			continue
		}
		if strings.HasPrefix(f.Value, "@") {
			path := strings.TrimPrefix(f.Value, "@")
			if err := addFile(w, f.Name, path); err != nil {
				return nil, "", err
			}
			continue
		}
		if err := w.WriteField(f.Name, f.Value); err != nil {
			return nil, "", err
		}
	}
	if err := w.Close(); err != nil {
		return nil, "", err
	}
	return buf.Bytes(), w.FormDataContentType(), nil
}

func addFile(w *multipart.Writer, fieldName, path string) error {
	f, err := os.Open(path)
	if err != nil {
		return fmt.Errorf("multipart: open %s: %w", path, err)
	}
	defer f.Close()
	part, err := w.CreateFormFile(fieldName, filepath.Base(path))
	if err != nil {
		return err
	}
	_, err = io.Copy(part, f)
	return err
}
