package httpfile

// Request represents a parsed .http file.
type Request struct {
	Method  string
	URL     string
	Headers map[string]string
	Body    string
	RawText string
	// Vars carries inline variables declared by the source file (e.g. Bruno
	// `vars:pre-request` blocks). They override env-file and collection vars
	// during resolution.
	Vars map[string]string
}
