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

// Find walks up the directory tree from dir to root (inclusive), collecting
// .env and .env.* files at each level. When the same label appears at multiple
// levels the deepest (most specific) file wins. Returns files sorted by label.
func Find(dir, root string) []EnvFile {
	dir = filepath.Clean(dir)
	root = filepath.Clean(root)

	// Walk from dir up to root, collecting dirs in order (deepest first).
	var dirs []string
	cur := dir
	for {
		dirs = append(dirs, cur)
		if cur == root {
			break
		}
		parent := filepath.Dir(cur)
		if parent == cur {
			// Reached filesystem root without hitting root — stop.
			break
		}
		cur = parent
	}

	// Merge: deepest wins. Use a map keyed by label so shallower levels don't
	// overwrite entries already found at a deeper level.
	seen := map[string]bool{}
	var result []EnvFile
	for _, d := range dirs {
		entries, err := os.ReadDir(d)
		if err != nil {
			continue
		}
		for _, e := range entries {
			if e.IsDir() {
				continue
			}
			name := e.Name()
			var label string
			switch {
			case name == ".env":
				label = "default"
			case strings.HasPrefix(name, ".env."):
				label = strings.TrimPrefix(name, ".env.")
			default:
				continue
			}
			if seen[label] {
				continue
			}
			seen[label] = true
			result = append(result, EnvFile{Path: filepath.Join(d, name), Label: label})
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
