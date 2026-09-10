// Package curlfile imports/exports HTTP requests as curl one-liners.
//
// Import handles the common subset of curl flags people actually paste:
// -X/--request, -H/--header, -d/--data/--data-raw/--data-binary,
// --data-urlencode, -u/--user, --url, -F/--form (treated as multipart).
//
// As curl itself does, a request built from -d/--data* fields defaults to
// Content-Type: application/x-www-form-urlencoded unless one is set explicitly.
//
// Anything more exotic (--cert, --cacert, --resolve, --proxy, etc.) is silently
// dropped — the goal is best-effort, not full curl emulation.
package curlfile

import (
	"encoding/base64"
	"fmt"
	"net/url"
	"sort"
	"strings"

	"github.com/bgunnarsson/binman/internal/httpfile"
)

// IsCurl reports whether s looks like a curl invocation.
func IsCurl(s string) bool {
	t := strings.TrimSpace(s)
	return strings.HasPrefix(t, "curl ") || strings.HasPrefix(t, "curl\t") ||
		strings.HasPrefix(t, "curl\n") || t == "curl" || strings.HasPrefix(t, "$ curl ")
}

// Parse converts a curl command-line string into a Request.
func Parse(s string) (*httpfile.Request, error) {
	tokens, err := tokenize(s)
	if err != nil {
		return nil, err
	}
	if len(tokens) == 0 || (tokens[0] != "curl" && tokens[0] != "$") {
		return nil, fmt.Errorf("not a curl command")
	}
	// strip a leading "$ curl"
	if tokens[0] == "$" && len(tokens) > 1 && tokens[1] == "curl" {
		tokens = tokens[2:]
	} else {
		tokens = tokens[1:]
	}

	req := &httpfile.Request{Headers: map[string]string{}}
	method := ""
	var positional []string
	// Track whether any -d/--data/--data-urlencode field was seen so we can
	// mirror curl's implicit application/x-www-form-urlencoded content type.
	sawFormData := false

	for i := 0; i < len(tokens); i++ {
		tok := tokens[i]
		next := func() (string, bool) {
			if i+1 >= len(tokens) {
				return "", false
			}
			i++
			return tokens[i], true
		}
		switch {
		case tok == "-X" || tok == "--request":
			if v, ok := next(); ok {
				method = strings.ToUpper(v)
			}
		case tok == "-H" || tok == "--header":
			if v, ok := next(); ok {
				if idx := strings.Index(v, ":"); idx > 0 {
					req.Headers[strings.TrimSpace(v[:idx])] = strings.TrimSpace(v[idx+1:])
				}
			}
		case tok == "-d" || tok == "--data" || tok == "--data-raw" || tok == "--data-binary":
			if v, ok := next(); ok {
				if req.Body != "" {
					req.Body += "&"
				}
				req.Body += v
				sawFormData = true
				if method == "" {
					method = "POST"
				}
			}
		case tok == "--data-urlencode":
			if v, ok := next(); ok {
				if req.Body != "" {
					req.Body += "&"
				}
				if eq := strings.Index(v, "="); eq >= 0 {
					req.Body += v[:eq+1] + url.QueryEscape(v[eq+1:])
				} else {
					req.Body += url.QueryEscape(v)
				}
				sawFormData = true
				if method == "" {
					method = "POST"
				}
			}
		case tok == "-u" || tok == "--user":
			if v, ok := next(); ok {
				req.Headers["Authorization"] = "Basic " + base64.StdEncoding.EncodeToString([]byte(v))
			}
		case tok == "--url":
			if v, ok := next(); ok {
				req.URL = v
			}
		case tok == "-F" || tok == "--form":
			// Note as a header so the user knows; full multipart parse is out of scope.
			if v, ok := next(); ok {
				if req.Body != "" {
					req.Body += "&"
				}
				req.Body += v
				if method == "" {
					method = "POST"
				}
				if _, exists := req.Headers["Content-Type"]; !exists {
					req.Headers["Content-Type"] = "multipart/form-data"
				}
			}
		case strings.HasPrefix(tok, "-"):
			// Unknown flag with arg — try to swallow the next token if it doesn't look like a flag.
			if i+1 < len(tokens) && !strings.HasPrefix(tokens[i+1], "-") && !looksLikeURL(tokens[i+1]) {
				i++
			}
		default:
			positional = append(positional, tok)
		}
	}

	for _, p := range positional {
		if req.URL == "" {
			req.URL = p
		}
	}
	if method == "" {
		method = "GET"
	}
	req.Method = method
	// curl implicitly sends application/x-www-form-urlencoded for -d/--data
	// fields. Preserve that so form bodies (e.g. OAuth2 client_credentials token
	// requests) are recognized as forms and sent with the right content type.
	if sawFormData {
		if _, exists := req.Headers["Content-Type"]; !exists {
			req.Headers["Content-Type"] = "application/x-www-form-urlencoded"
		}
	}
	return req, nil
}

func looksLikeURL(s string) bool {
	return strings.HasPrefix(s, "http://") || strings.HasPrefix(s, "https://") ||
		strings.HasPrefix(s, "{{")
}

// Format renders req as a single-line curl command suitable for clipboard paste.
func Format(req *httpfile.Request) string {
	var b strings.Builder
	b.WriteString("curl")
	method := req.Method
	if method == "" {
		method = "GET"
	}
	if method != "GET" {
		b.WriteString(" -X ")
		b.WriteString(method)
	}
	keys := make([]string, 0, len(req.Headers))
	for k := range req.Headers {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		fmt.Fprintf(&b, " -H %s", shellQuote(k+": "+req.Headers[k]))
	}
	if req.Body != "" {
		fmt.Fprintf(&b, " --data-raw %s", shellQuote(req.Body))
	}
	if req.URL != "" {
		b.WriteByte(' ')
		b.WriteString(shellQuote(req.URL))
	}
	return b.String()
}

func shellQuote(s string) string {
	if !strings.ContainsAny(s, " \t\n'\"\\$`!*?[]()") {
		return s
	}
	return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'"
}

// tokenize is a minimal POSIX-ish shell tokenizer that handles single quotes,
// double quotes, backslash-escapes, and line-continuation (`\` + newline).
// It is intentionally not complete — it covers what curl pastes look like.
func tokenize(s string) ([]string, error) {
	var (
		out      []string
		cur      strings.Builder
		inSingle bool
		inDouble bool
	)
	flush := func() {
		if cur.Len() > 0 {
			out = append(out, cur.String())
			cur.Reset()
		}
	}
	for i := 0; i < len(s); i++ {
		c := s[i]
		switch {
		case inSingle:
			if c == '\'' {
				inSingle = false
				continue
			}
			cur.WriteByte(c)
		case inDouble:
			if c == '"' {
				inDouble = false
				continue
			}
			if c == '\\' && i+1 < len(s) {
				next := s[i+1]
				if next == '"' || next == '\\' || next == '$' || next == '`' || next == '\n' {
					cur.WriteByte(next)
					i++
					continue
				}
			}
			cur.WriteByte(c)
		default:
			switch c {
			case '\'':
				inSingle = true
			case '"':
				inDouble = true
			case '\\':
				if i+1 < len(s) {
					next := s[i+1]
					if next == '\n' {
						i++
						continue
					}
					cur.WriteByte(next)
					i++
				}
			case ' ', '\t', '\n', '\r':
				flush()
			default:
				cur.WriteByte(c)
			}
		}
	}
	if inSingle || inDouble {
		return nil, fmt.Errorf("unterminated quote")
	}
	flush()
	return out, nil
}
