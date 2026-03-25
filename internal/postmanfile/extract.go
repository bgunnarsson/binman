package postmanfile

import (
	"fmt"
	"strings"

	"github.com/bgunnarsson/binman/internal/httpfile"
)

// RequestAt walks the collection's item tree by itemPath (slice of indices)
// and returns the matched request as an httpfile.Request.
func RequestAt(c *Collection, itemPath []int) (*httpfile.Request, error) {
	items := c.Items
	var item Item
	for depth, idx := range itemPath {
		if idx < 0 || idx >= len(items) {
			return nil, fmt.Errorf("postmanfile: index %d out of range at depth %d", idx, depth)
		}
		item = items[idx]
		items = item.Items
	}

	if item.Request == nil {
		return nil, fmt.Errorf("postmanfile: item at path %v is a folder, not a request", itemPath)
	}

	r := item.Request
	req := &httpfile.Request{
		Method:  strings.ToUpper(r.Method),
		URL:     r.URL.Raw,
		Headers: make(map[string]string),
	}

	for _, h := range r.Header {
		if !h.Disabled {
			req.Headers[h.Key] = h.Value
		}
	}

	if r.Body != nil && r.Body.Mode == "raw" {
		req.Body = r.Body.Raw
	}

	return req, nil
}
