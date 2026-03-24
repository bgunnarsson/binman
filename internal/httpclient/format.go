package httpclient

import (
	"bytes"
	"encoding/json"
	"strings"
)

// FormatBody attempts to pretty-print JSON; returns raw text otherwise.
func FormatBody(body string) string {
	trimmed := strings.TrimSpace(body)
	if len(trimmed) > 0 && (trimmed[0] == '{' || trimmed[0] == '[') {
		var buf bytes.Buffer
		if err := json.Indent(&buf, []byte(trimmed), "", "  "); err == nil {
			return buf.String()
		}
	}
	return body
}

// HighlightBody pretty-prints JSON and returns tview color-markup text for
// display in a TextView with SetDynamicColors(true). Non-JSON is returned
// with tview special characters escaped.
func HighlightBody(body string) string {
	trimmed := strings.TrimSpace(body)
	if len(trimmed) > 0 && (trimmed[0] == '{' || trimmed[0] == '[') {
		var buf bytes.Buffer
		if err := json.Indent(&buf, []byte(trimmed), "", "  "); err == nil {
			return colorizeJSON(buf.String())
		}
	}
	return strings.NewReplacer(`[`, `["`, `]`, `["]`).Replace(body)
}

// colorizeJSON converts pretty-printed JSON into tview color-markup text.
func colorizeJSON(s string) string {
	runes := []rune(s)
	n := len(runes)
	var buf strings.Builder
	buf.Grow(n * 3)

	i := 0
	for i < n {
		ch := runes[i]

		// Strings
		if ch == '"' {
			if jsonStringIsKey(runes, i) {
				buf.WriteString(`[#a78bfa]`)
			} else {
				buf.WriteString(`[#86efac]`)
			}
			buf.WriteRune('"')
			i++
			for i < n {
				c := runes[i]
				if c == '\\' {
					buf.WriteRune(c)
					i++
					if i < n {
						writeTview(&buf, runes[i])
						i++
					}
					continue
				}
				if c == '"' {
					buf.WriteRune('"')
					i++
					break
				}
				writeTview(&buf, c)
				i++
			}
			buf.WriteString(`[-]`)
			continue
		}

		// Keywords
		if ch == 't' && i+4 <= n && string(runes[i:i+4]) == "true" {
			buf.WriteString(`[#93c5fd]true[-]`)
			i += 4
			continue
		}
		if ch == 'f' && i+5 <= n && string(runes[i:i+5]) == "false" {
			buf.WriteString(`[#93c5fd]false[-]`)
			i += 5
			continue
		}
		if ch == 'n' && i+4 <= n && string(runes[i:i+4]) == "null" {
			buf.WriteString(`[#6b7090]null[-]`)
			i += 4
			continue
		}

		// Numbers
		if ch == '-' || (ch >= '0' && ch <= '9') {
			j := i
			for j < n {
				c := runes[j]
				if c == '-' || c == '+' || c == '.' || c == 'e' || c == 'E' || (c >= '0' && c <= '9') {
					j++
				} else {
					break
				}
			}
			buf.WriteString(`[#fcd34d]`)
			buf.WriteString(string(runes[i:j]))
			buf.WriteString(`[-]`)
			i = j
			continue
		}

		// Punctuation
		switch ch {
		case '{', '}', ',', ':':
			buf.WriteString(`[#4a4f72]`)
			buf.WriteRune(ch)
			buf.WriteString(`[-]`)
		case '[':
			// ["  = tview escape for literal [
			buf.WriteString(`[#4a4f72]["[-]`)
		case ']':
			// ] after a closed tag is a literal ]
			buf.WriteString(`[#4a4f72]][-]`)
		default:
			buf.WriteRune(ch)
		}
		i++
	}

	return buf.String()
}

// writeTview writes r to buf, escaping [ for tview dynamic color parsing.
func writeTview(buf *strings.Builder, r rune) {
	if r == '[' {
		buf.WriteString(`["`)
		return
	}
	buf.WriteRune(r)
}

// jsonStringIsKey reports whether the JSON string starting at runes[start]
// is an object key (i.e. followed by ':' after its closing quote).
func jsonStringIsKey(runes []rune, start int) bool {
	n := len(runes)
	i := start + 1
	for i < n {
		if runes[i] == '\\' {
			i += 2
			continue
		}
		if runes[i] == '"' {
			i++
			for i < n && (runes[i] == ' ' || runes[i] == '\t') {
				i++
			}
			return i < n && runes[i] == ':'
		}
		i++
	}
	return false
}
