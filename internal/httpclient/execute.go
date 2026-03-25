package httpclient

import (
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	"github.com/bgunnarsson/binman/internal/httpfile"
)

// Response holds the result of an HTTP request.
type Response struct {
	StatusCode int
	Status     string
	Headers    http.Header
	Body       string
	Duration   time.Duration
	Err        error
}

// Execute sends the HTTP request and returns the response.
// It never returns a non-nil error; errors are stored in Response.Err.
func Execute(req *httpfile.Request) *Response {
	method := req.Method
	if method == "" {
		method = "GET"
	}

	var bodyReader io.Reader
	if req.Body != "" {
		bodyReader = strings.NewReader(req.Body)
	}

	httpReq, err := http.NewRequest(method, req.URL, bodyReader)
	if err != nil {
		return &Response{Err: fmt.Errorf("creating request: %w", err)}
	}

	for k, v := range req.Headers {
		httpReq.Header.Set(k, v)
	}

	start := time.Now()
	resp, err := DefaultClient.Do(httpReq)
	elapsed := time.Since(start)
	if err != nil {
		return &Response{Err: fmt.Errorf("sending request: %w", err), Duration: elapsed}
	}
	defer resp.Body.Close()

	bodyBytes, err := io.ReadAll(resp.Body)
	if err != nil {
		return &Response{
			StatusCode: resp.StatusCode,
			Status:     resp.Status,
			Headers:    resp.Header,
			Duration:   elapsed,
			Err:        fmt.Errorf("reading response body: %w", err),
		}
	}

	return &Response{
		StatusCode: resp.StatusCode,
		Status:     resp.Status,
		Headers:    resp.Header,
		Body:       string(bodyBytes),
		Duration:   elapsed,
	}
}
