package httpfile

// Request represents a parsed .http file.
type Request struct {
	Method  string
	URL     string
	Headers map[string]string
	Body    string
	RawText string
}
