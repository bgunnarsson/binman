package httpfile

import "os"

// Load reads a .http file and parses it.
func Load(path string) (*Request, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	return Parse(string(b))
}
