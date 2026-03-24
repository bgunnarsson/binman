package httpclient

import (
	"net/http"
	"time"
)

// DefaultClient is used for all requests.
var DefaultClient = &http.Client{
	Timeout: 30 * time.Second,
}
