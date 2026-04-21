package ui

import (
	"fmt"
	"net/http"
	"sort"
	"strings"
	"time"

	"github.com/bgunnarsson/binman/internal/httpclient"
)

// UpdateCookies populates the Cookies response tab with both the Set-Cookie
// values returned on this response and the persisted jar contents for the URL.
func (v *View) UpdateCookies(setCookies []*http.Cookie, jarCookies []*http.Cookie) {
	var b strings.Builder

	if len(setCookies) > 0 {
		b.WriteString("[#a78bfa]Set on this response[-]\n")
		for _, c := range setCookies {
			fmt.Fprintf(&b, "  [#86efac]%s[-]=[#d4d8e8]%s[-]", c.Name, c.Value)
			var attrs []string
			if c.Path != "" {
				attrs = append(attrs, "Path="+c.Path)
			}
			if c.Domain != "" {
				attrs = append(attrs, "Domain="+c.Domain)
			}
			if !c.Expires.IsZero() {
				attrs = append(attrs, "Expires="+c.Expires.Format(time.RFC3339))
			}
			if c.MaxAge > 0 {
				attrs = append(attrs, fmt.Sprintf("Max-Age=%d", c.MaxAge))
			}
			if c.HttpOnly {
				attrs = append(attrs, "HttpOnly")
			}
			if c.Secure {
				attrs = append(attrs, "Secure")
			}
			if len(attrs) > 0 {
				fmt.Fprintf(&b, "  [#6b7090](%s)[-]", strings.Join(attrs, "; "))
			}
			b.WriteByte('\n')
		}
		b.WriteByte('\n')
	}

	if len(jarCookies) > 0 {
		b.WriteString("[#a78bfa]Jar (will be sent on next request)[-]\n")
		for _, c := range jarCookies {
			fmt.Fprintf(&b, "  [#86efac]%s[-]=[#d4d8e8]%s[-]\n", c.Name, c.Value)
		}
	}

	if b.Len() == 0 {
		v.RespCookiesTv.SetText("\n  [#4a4f72]No cookies[-]")
		return
	}
	v.RespCookiesTv.SetText(b.String())
}

// UpdateTrace renders the request timing breakdown.
func (v *View) UpdateTrace(t httpclient.Trace) {
	if t.Total == 0 {
		v.RespTraceTv.SetText("\n  [#4a4f72]No trace data[-]")
		return
	}
	row := func(label string, d time.Duration) string {
		if d == 0 {
			return fmt.Sprintf("  [#6b7090]%-12s[-] [#4a4f72]—[-]\n", label)
		}
		return fmt.Sprintf("  [#a78bfa]%-12s[-] [#d4d8e8]%s[-]\n", label, d.Round(time.Microsecond))
	}
	var b strings.Builder
	b.WriteString(row("DNS", t.DNS))
	b.WriteString(row("Connect", t.Connect))
	b.WriteString(row("TLS", t.TLS))
	b.WriteString(row("TTFB", t.TTFB))
	b.WriteString(row("Total", t.Total))
	v.RespTraceTv.SetText(b.String())
}

// UpdateScriptsResult shows the values extracted from the response by the
// Scripts-tab rules. Empty extracted map renders a placeholder.
func (v *View) UpdateScriptsResult(extracted map[string]string) {
	if len(extracted) == 0 {
		v.RespScriptsTv.SetText("\n  [#4a4f72]No variables extracted.\n  Add rules in the request Scripts tab, e.g.\n    TOKEN = json data.token\n    ID = regex /id=(\\d+)/\n    SERVER = header Server[-]")
		return
	}
	keys := make([]string, 0, len(extracted))
	for k := range extracted {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	var b strings.Builder
	b.WriteString("[#a78bfa]Extracted variables[-]\n")
	for _, k := range keys {
		fmt.Fprintf(&b, "  [#86efac]%s[-] = [#d4d8e8]%s[-]\n", k, extracted[k])
	}
	v.RespScriptsTv.SetText(b.String())
}
