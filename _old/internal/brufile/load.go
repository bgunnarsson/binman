package brufile

import (
	"os"

	"github.com/bgunnarsson/binman/internal/httpfile"
)

// Load reads a .bru file from disk and parses it into an httpfile.Request.
func Load(path string) (*httpfile.Request, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	return Parse(string(data))
}
