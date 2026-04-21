package httpclient

import (
	"bufio"
	"context"
	"crypto/tls"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptrace"
	"net/textproto"
	"strings"
	"time"

	"github.com/bgunnarsson/binman/internal/httpfile"
)

// Trace holds per-phase timings for a single request.
type Trace struct {
	DNS     time.Duration
	Connect time.Duration
	TLS     time.Duration
	TTFB    time.Duration
	Total   time.Duration
}

// Response holds the result of an HTTP request.
type Response struct {
	StatusCode int
	Status     string
	Headers    http.Header
	Body       string
	Cookies    []*http.Cookie // cookies set on this response
	Duration   time.Duration
	Trace      Trace
	Stream     bool // true if Body was streamed via callback (SSE)
	Err        error
}

// StreamHandler receives one event line at a time for streaming responses.
// Return false to stop reading further events.
type StreamHandler func(event string) bool

// Execute sends the HTTP request and returns the response.
// It never returns a non-nil error; errors are stored in Response.Err.
// If ctx is cancelled mid-flight the response Err is the context error.
// onStream, when non-nil and the response is text/event-stream, receives each
// SSE event chunk as it arrives; the final Body will hold all events joined.
func Execute(ctx context.Context, req *httpfile.Request, onStream StreamHandler) *Response {
	method := req.Method
	if method == "" {
		method = "GET"
	}

	var bodyReader io.Reader
	if req.Body != "" {
		bodyReader = strings.NewReader(req.Body)
	}

	httpReq, err := http.NewRequestWithContext(ctx, method, req.URL, bodyReader)
	if err != nil {
		return &Response{Err: fmt.Errorf("creating request: %w", err)}
	}

	for k, v := range req.Headers {
		httpReq.Header.Set(k, v)
	}

	tr := &Trace{}
	var dnsStart, connStart, tlsStart, reqStart time.Time
	trace := &httptrace.ClientTrace{
		DNSStart:          func(httptrace.DNSStartInfo) { dnsStart = time.Now() },
		DNSDone:           func(httptrace.DNSDoneInfo) { tr.DNS = time.Since(dnsStart) },
		ConnectStart:      func(_, _ string) { connStart = time.Now() },
		ConnectDone:       func(_, _ string, _ error) { tr.Connect = time.Since(connStart) },
		TLSHandshakeStart: func() { tlsStart = time.Now() },
		TLSHandshakeDone: func(_ tls.ConnectionState, _ error) {
			if !tlsStart.IsZero() {
				tr.TLS = time.Since(tlsStart)
			}
		},
		GotFirstResponseByte: func() { tr.TTFB = time.Since(reqStart) },
	}
	httpReq = httpReq.WithContext(httptrace.WithClientTrace(httpReq.Context(), trace))

	reqStart = time.Now()
	resp, err := DefaultClient.Do(httpReq)
	elapsed := time.Since(reqStart)
	tr.Total = elapsed
	if err != nil {
		// Surface ctx cancellation as the actual ctx error so callers can detect it.
		if errors.Is(ctx.Err(), context.Canceled) {
			return &Response{Err: context.Canceled, Duration: elapsed, Trace: *tr}
		}
		return &Response{Err: fmt.Errorf("sending request: %w", err), Duration: elapsed, Trace: *tr}
	}
	defer resp.Body.Close()

	out := &Response{
		StatusCode: resp.StatusCode,
		Status:     resp.Status,
		Headers:    resp.Header,
		Cookies:    resp.Cookies(),
		Duration:   elapsed,
		Trace:      *tr,
	}

	contentType := strings.ToLower(resp.Header.Get("Content-Type"))
	if onStream != nil && strings.HasPrefix(contentType, "text/event-stream") {
		out.Stream = true
		var collected strings.Builder
		// SSE events are separated by blank lines; emit per event block.
		scanner := bufio.NewReader(resp.Body)
		var event strings.Builder
		for {
			line, err := scanner.ReadString('\n')
			if line != "" {
				event.WriteString(line)
				if textproto.TrimString(line) == "" && event.Len() > 0 {
					// blank line — end of event
					ev := event.String()
					collected.WriteString(ev)
					if !onStream(ev) {
						break
					}
					event.Reset()
				}
			}
			if err != nil {
				if event.Len() > 0 {
					ev := event.String()
					collected.WriteString(ev)
					onStream(ev)
				}
				break
			}
		}
		out.Body = collected.String()
		out.Trace.Total = time.Since(reqStart)
		out.Duration = out.Trace.Total
		return out
	}

	bodyBytes, err := io.ReadAll(resp.Body)
	if err != nil {
		out.Err = fmt.Errorf("reading response body: %w", err)
		return out
	}
	out.Body = string(bodyBytes)
	out.Trace.Total = time.Since(reqStart)
	out.Duration = out.Trace.Total
	return out
}
