package httpclient

import (
	"crypto/tls"
	"net/http"
	"net/http/cookiejar"
	"net/url"
	"time"
)

// DefaultClient is used for all requests. It carries a cookie jar so stateful
// flows (login → call) work without manual Set-Cookie wiring.
var DefaultClient = newClient(30*time.Second, nil)

// Configure replaces DefaultClient with one built from the given options.
// Pass timeout=0 for no timeout (useful for SSE / long-poll endpoints).
// tlsConfig may be nil to use the default transport TLS settings.
func Configure(timeout time.Duration, tlsConfig *tls.Config) {
	DefaultClient = newClient(timeout, tlsConfig)
}

func newClient(timeout time.Duration, tlsConfig *tls.Config) *http.Client {
	jar, _ := cookiejar.New(nil)
	transport := http.DefaultTransport.(*http.Transport).Clone()
	if tlsConfig != nil {
		transport.TLSClientConfig = tlsConfig
	}
	return &http.Client{
		Timeout:   timeout,
		Jar:       jar,
		Transport: transport,
	}
}

// CookiesFor returns the cookies currently held for the given URL.
func CookiesFor(rawURL string) []*http.Cookie {
	if DefaultClient.Jar == nil {
		return nil
	}
	u, err := url.Parse(rawURL)
	if err != nil {
		return nil
	}
	return DefaultClient.Jar.Cookies(u)
}
