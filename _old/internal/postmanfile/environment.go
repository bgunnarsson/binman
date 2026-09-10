package postmanfile

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
)

// NamedEnvFile is a discovered *.postman_environment.json file.
type NamedEnvFile struct {
	Label string
	Path  string
}

// Environment is the JSON shape exported by Postman for an environment.
// values[].enabled defaults to true when omitted.
type Environment struct {
	Name   string         `json:"name"`
	Values []EnvironmentV `json:"values"`
}

// EnvironmentV is a single key/value entry in a Postman environment.
type EnvironmentV struct {
	Key     string `json:"key"`
	Value   string `json:"value"`
	Enabled *bool  `json:"enabled,omitempty"`
}

// ParseEnvironment unmarshals a Postman environment file and returns its
// key→value map, honoring the `enabled` flag.
func ParseEnvironment(data []byte) (map[string]string, error) {
	var env Environment
	if err := json.Unmarshal(data, &env); err != nil {
		return nil, err
	}
	if len(env.Values) == 0 {
		return nil, nil
	}
	out := make(map[string]string, len(env.Values))
	for _, v := range env.Values {
		if v.Enabled != nil && !*v.Enabled {
			continue
		}
		out[v.Key] = v.Value
	}
	return out, nil
}

// LoadEnvironment reads and parses a *.postman_environment.json file.
func LoadEnvironment(path string) (map[string]string, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	return ParseEnvironment(data)
}

// FindEnvironments walks up from dir to root looking for *.postman_environment.json
// files. Files found at deeper levels override matching labels at shallower
// levels (same precedence as .env discovery).
func FindEnvironments(dir, root string) []NamedEnvFile {
	dir = filepath.Clean(dir)
	root = filepath.Clean(root)

	var dirs []string
	cur := dir
	for {
		dirs = append(dirs, cur)
		if cur == root {
			break
		}
		parent := filepath.Dir(cur)
		if parent == cur {
			break
		}
		cur = parent
	}

	seen := map[string]bool{}
	var out []NamedEnvFile
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
			if !strings.HasSuffix(strings.ToLower(name), ".postman_environment.json") {
				continue
			}
			label := strings.TrimSuffix(name, ".postman_environment.json")
			label = strings.TrimSuffix(label, ".postman_environment.JSON")
			if seen[label] {
				continue
			}
			seen[label] = true
			out = append(out, NamedEnvFile{
				Label: label,
				Path:  filepath.Join(d, name),
			})
		}
	}
	return out
}
