package brufile

import (
	"os"
	"path/filepath"
	"strings"
)

// NamedVars is a labeled variable set, used to surface Bruno environments and
// the like in a UI selector.
type NamedVars struct {
	Label string
	Path  string
}

// CollectionVars walks up from dir to root (inclusive) accumulating `vars`
// blocks from any collection.bru or folder.bru files it finds. Shallower files
// (collection.bru at the collection root) form the base; deeper files
// (folder.bru nested inside) override matching keys.
func CollectionVars(dir, root string) map[string]string {
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

	// Visit shallowest first so deeper folder.bru wins for any given key.
	merged := map[string]string{}
	for i := len(dirs) - 1; i >= 0; i-- {
		for _, name := range []string{"collection.bru", "folder.bru"} {
			path := filepath.Join(dirs[i], name)
			data, err := os.ReadFile(path)
			if err != nil {
				continue
			}
			for k, v := range ParseVarsBlocks(string(data)) {
				merged[k] = v
			}
		}
	}
	if len(merged) == 0 {
		return nil
	}
	return merged
}

// FindEnvironments locates Bruno environment files by walking up from dir to
// root looking for an `environments/` directory containing *.bru files. The
// first directory found wins (Bruno collections only have one environments
// folder, at the collection root).
func FindEnvironments(dir, root string) []NamedVars {
	dir = filepath.Clean(dir)
	root = filepath.Clean(root)

	cur := dir
	for {
		envDir := filepath.Join(cur, "environments")
		if entries, err := os.ReadDir(envDir); err == nil {
			var out []NamedVars
			for _, e := range entries {
				if e.IsDir() {
					continue
				}
				name := e.Name()
				if !strings.EqualFold(filepath.Ext(name), ".bru") {
					continue
				}
				label := strings.TrimSuffix(name, filepath.Ext(name))
				out = append(out, NamedVars{
					Label: label,
					Path:  filepath.Join(envDir, name),
				})
			}
			if len(out) > 0 {
				return out
			}
		}
		if cur == root {
			break
		}
		parent := filepath.Dir(cur)
		if parent == cur {
			break
		}
		cur = parent
	}
	return nil
}

// LoadEnvironment reads an environments/*.bru file and returns its vars.
func LoadEnvironment(path string) (map[string]string, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	return ParseVarsBlocks(string(data)), nil
}
