// Package extract pulls values out of HTTP response bodies into named variables
// using a tiny rule format. Rules are written in the request's Scripts tab, one
// per line, in the form:
//
//	NAME = json.dotted.path
//	NAME = regex /pattern/        (first capture group)
//	NAME = header Some-Header     (response header value)
//
// Lines starting with # are comments. Empty lines are ignored. Unknown rule
// types are skipped.
package extract

import (
	"encoding/json"
	"net/http"
	"regexp"
	"strconv"
	"strings"
)

// Rule is one parsed extraction directive.
type Rule struct {
	Name string
	Kind string // "json", "regex", "header"
	Arg  string
}

// ParseRules parses the Scripts-tab text into a list of rules.
func ParseRules(script string) []Rule {
	var rules []Rule
	for _, line := range strings.Split(script, "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		eq := strings.Index(line, "=")
		if eq < 0 {
			continue
		}
		name := strings.TrimSpace(line[:eq])
		rest := strings.TrimSpace(line[eq+1:])
		if name == "" || rest == "" {
			continue
		}
		fields := strings.SplitN(rest, " ", 2)
		if len(fields) != 2 {
			continue
		}
		kind := strings.ToLower(fields[0])
		arg := strings.TrimSpace(fields[1])
		switch kind {
		case "json", "regex", "header":
			rules = append(rules, Rule{Name: name, Kind: kind, Arg: arg})
		}
	}
	return rules
}

// Apply runs the rules against the given response body / headers and returns
// a map of variable name → extracted value. Failed extractions are omitted.
func Apply(rules []Rule, body string, headers http.Header) map[string]string {
	out := map[string]string{}
	var jsonRoot any
	jsonParsed := false

	for _, r := range rules {
		switch r.Kind {
		case "json":
			if !jsonParsed {
				_ = json.Unmarshal([]byte(body), &jsonRoot)
				jsonParsed = true
			}
			if v, ok := walkJSON(jsonRoot, r.Arg); ok {
				out[r.Name] = v
			}
		case "regex":
			pattern := strings.Trim(r.Arg, "/")
			re, err := regexp.Compile(pattern)
			if err != nil {
				continue
			}
			m := re.FindStringSubmatch(body)
			if len(m) >= 2 {
				out[r.Name] = m[1]
			} else if len(m) == 1 {
				out[r.Name] = m[0]
			}
		case "header":
			if headers != nil {
				if v := headers.Get(r.Arg); v != "" {
					out[r.Name] = v
				}
			}
		}
	}
	return out
}

// walkJSON resolves a dotted path like "data.items.0.id" against a parsed
// JSON value. Numeric segments index into arrays.
func walkJSON(root any, path string) (string, bool) {
	cur := root
	for _, seg := range strings.Split(path, ".") {
		if seg == "" {
			continue
		}
		switch v := cur.(type) {
		case map[string]any:
			next, ok := v[seg]
			if !ok {
				return "", false
			}
			cur = next
		case []any:
			idx, err := strconv.Atoi(seg)
			if err != nil || idx < 0 || idx >= len(v) {
				return "", false
			}
			cur = v[idx]
		default:
			return "", false
		}
	}
	return stringify(cur)
}

func stringify(v any) (string, bool) {
	switch x := v.(type) {
	case nil:
		return "", false
	case string:
		return x, true
	case bool:
		if x {
			return "true", true
		}
		return "false", true
	case float64:
		// JSON numbers come back as float64; render integers without trailing zero.
		if x == float64(int64(x)) {
			return strconv.FormatInt(int64(x), 10), true
		}
		return strconv.FormatFloat(x, 'g', -1, 64), true
	default:
		b, err := json.Marshal(v)
		if err != nil {
			return "", false
		}
		return string(b), true
	}
}
