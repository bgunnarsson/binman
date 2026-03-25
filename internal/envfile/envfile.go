package envfile

import (
	"bufio"
	"os"
	"path/filepath"
	"regexp"
	"strings"
)

// EnvFile represents a discovered .env file with a short display label.
type EnvFile struct {
	Path  string
	Label string
}

// Find scans dir for .env and .env.* files.
func Find(dir string) []EnvFile {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}
	var result []EnvFile
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		name := e.Name()
		switch {
		case name == ".env":
			result = append(result, EnvFile{Path: filepath.Join(dir, name), Label: "default"})
		case strings.HasPrefix(name, ".env."):
			label := strings.TrimPrefix(name, ".env.")
			result = append(result, EnvFile{Path: filepath.Join(dir, name), Label: label})
		}
	}
	return result
}

// Parse reads KEY=VALUE pairs from a .env file.
func Parse(path string) (map[string]string, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()

	vars := make(map[string]string)
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		line := strings.TrimSpace(sc.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		idx := strings.IndexByte(line, '=')
		if idx < 0 {
			continue
		}
		key := strings.TrimSpace(line[:idx])
		val := strings.TrimSpace(line[idx+1:])
		vars[key] = val
	}
	return vars, sc.Err()
}

var varRe = regexp.MustCompile(`\{\{([^}]+)\}\}`)

// Resolve replaces {{VAR}} placeholders in s using vars. Unmatched placeholders are left as-is.
func Resolve(s string, vars map[string]string) string {
	if len(vars) == 0 {
		return s
	}
	return varRe.ReplaceAllStringFunc(s, func(match string) string {
		key := strings.TrimSpace(match[2 : len(match)-2])
		if val, ok := vars[key]; ok {
			return val
		}
		return match
	})
}
