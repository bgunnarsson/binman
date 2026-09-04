package oauth2

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

// The token cache is package-level, so every test that wants a cold start has
// to clear it. httptest hands out a fresh port per server, which keeps the keys
// of concurrent tests apart, but a test that asserts on a hit or a miss needs
// to know it started from nothing.
func resetCache(t *testing.T) {
	t.Helper()
	mu.Lock()
	cache = map[cacheKey]cachedToken{}
	mu.Unlock()
}

// tokenServer answers the client-credentials grant and records what it was
// sent, so tests can assert on the form the endpoint actually receives.
type tokenServer struct {
	*httptest.Server
	calls atomic.Int32
	form  chan map[string]string
}

func newTokenServer(t *testing.T, body func(call int32) (int, string)) *tokenServer {
	t.Helper()
	ts := &tokenServer{form: make(chan map[string]string, 8)}
	ts.Server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		call := ts.calls.Add(1)
		_ = r.ParseForm()
		seen := map[string]string{}
		for k := range r.PostForm {
			seen[k] = r.PostForm.Get(k)
		}
		seen["__method"] = r.Method
		seen["__content-type"] = r.Header.Get("Content-Type")
		seen["__accept"] = r.Header.Get("Accept")
		select {
		case ts.form <- seen:
		default:
		}
		status, payload := body(call)
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(status)
		fmt.Fprint(w, payload)
	}))
	t.Cleanup(ts.Close)
	return ts
}

func TestFetchSendsTheClientCredentialsGrant(t *testing.T) {
	resetCache(t)
	srv := newTokenServer(t, func(int32) (int, string) {
		return 200, `{"access_token":"tok-1","token_type":"Bearer","expires_in":3600}`
	})

	got, err := FetchClientCredentials(context.Background(), srv.URL, "id", "secret", "read write")
	if err != nil {
		t.Fatal(err)
	}
	if got != "tok-1" {
		t.Errorf("token: %q", got)
	}

	form := <-srv.form
	// The endpoint has to receive a form POST, not JSON — RFC 6749 §4.4.2.
	if form["__method"] != "POST" {
		t.Errorf("method: %q", form["__method"])
	}
	if !strings.HasPrefix(form["__content-type"], "application/x-www-form-urlencoded") {
		t.Errorf("content-type: %q", form["__content-type"])
	}
	if form["__accept"] != "application/json" {
		t.Errorf("accept: %q", form["__accept"])
	}
	for k, want := range map[string]string{
		"grant_type":    "client_credentials",
		"client_id":     "id",
		"client_secret": "secret",
		"scope":         "read write",
	} {
		if form[k] != want {
			t.Errorf("%s: got %q, want %q", k, form[k], want)
		}
	}
}

func TestFetchOmitsScopeWhenEmpty(t *testing.T) {
	resetCache(t)
	srv := newTokenServer(t, func(int32) (int, string) {
		return 200, `{"access_token":"tok","expires_in":60}`
	})

	if _, err := FetchClientCredentials(context.Background(), srv.URL, "id", "secret", ""); err != nil {
		t.Fatal(err)
	}
	// An empty `scope` is not the same as no scope: some providers reject it
	// outright, others read it as "no scopes at all" and issue a useless token.
	if form := <-srv.form; form["scope"] != "" || hasKey(form, "scope") {
		t.Errorf("scope was sent: %+v", form)
	}
}

func hasKey(m map[string]string, k string) bool {
	_, ok := m[k]
	return ok
}

func TestFetchCachesTheTokenAcrossCalls(t *testing.T) {
	resetCache(t)
	srv := newTokenServer(t, func(call int32) (int, string) {
		return 200, fmt.Sprintf(`{"access_token":"tok-%d","expires_in":3600}`, call)
	})

	first, err := FetchClientCredentials(context.Background(), srv.URL, "id", "secret", "")
	if err != nil {
		t.Fatal(err)
	}
	second, err := FetchClientCredentials(context.Background(), srv.URL, "id", "secret", "")
	if err != nil {
		t.Fatal(err)
	}
	if first != second {
		t.Errorf("second call got a different token: %q then %q", first, second)
	}
	if n := srv.calls.Load(); n != 1 {
		t.Errorf("token endpoint was hit %d times, want 1", n)
	}
}

func TestCacheIsKeyedOnClientAndScope(t *testing.T) {
	resetCache(t)
	srv := newTokenServer(t, func(call int32) (int, string) {
		return 200, fmt.Sprintf(`{"access_token":"tok-%d","expires_in":3600}`, call)
	})

	// Same endpoint, three different identities. A cache that ignored any part
	// of the key would hand the second caller the first one's token — which is
	// the failure mode that matters here, since the token is what authorises
	// the request that follows.
	a, _ := FetchClientCredentials(context.Background(), srv.URL, "id-a", "secret", "")
	b, _ := FetchClientCredentials(context.Background(), srv.URL, "id-b", "secret", "")
	c, _ := FetchClientCredentials(context.Background(), srv.URL, "id-a", "secret", "admin")
	if a == b || a == c || b == c {
		t.Errorf("tokens collided across cache keys: %q %q %q", a, b, c)
	}
	if n := srv.calls.Load(); n != 3 {
		t.Errorf("token endpoint was hit %d times, want 3", n)
	}
}

func TestAnExpiredTokenIsRefetched(t *testing.T) {
	resetCache(t)
	srv := newTokenServer(t, func(call int32) (int, string) {
		return 200, fmt.Sprintf(`{"access_token":"tok-%d","expires_in":3600}`, call)
	})

	first, err := FetchClientCredentials(context.Background(), srv.URL, "id", "secret", "")
	if err != nil {
		t.Fatal(err)
	}

	// Age the cached entry past its expiry rather than sleeping for an hour.
	mu.Lock()
	key := cacheKey{srv.URL, "id", ""}
	entry := cache[key]
	entry.expires = time.Now().Add(-time.Second)
	cache[key] = entry
	mu.Unlock()

	second, err := FetchClientCredentials(context.Background(), srv.URL, "id", "secret", "")
	if err != nil {
		t.Fatal(err)
	}
	if first == second {
		t.Errorf("expired token was reused: %q", second)
	}
	if n := srv.calls.Load(); n != 2 {
		t.Errorf("token endpoint was hit %d times, want 2", n)
	}
}

func TestATokenInsideTheSkewWindowIsRefetched(t *testing.T) {
	resetCache(t)
	srv := newTokenServer(t, func(call int32) (int, string) {
		return 200, fmt.Sprintf(`{"access_token":"tok-%d","expires_in":3600}`, call)
	})

	if _, err := FetchClientCredentials(context.Background(), srv.URL, "id", "secret", ""); err != nil {
		t.Fatal(err)
	}

	// Still valid, but inside the 30-second margin. Handing this one out would
	// mean a request that leaves here authorised and arrives expired.
	mu.Lock()
	key := cacheKey{srv.URL, "id", ""}
	entry := cache[key]
	entry.expires = time.Now().Add(10 * time.Second)
	cache[key] = entry
	mu.Unlock()

	if _, err := FetchClientCredentials(context.Background(), srv.URL, "id", "secret", ""); err != nil {
		t.Fatal(err)
	}
	if n := srv.calls.Load(); n != 2 {
		t.Errorf("token endpoint was hit %d times, want 2 — the skew window did not force a refetch", n)
	}
}

func TestAResponseWithoutExpiresInGetsTheDefaultHour(t *testing.T) {
	resetCache(t)
	srv := newTokenServer(t, func(int32) (int, string) {
		return 200, `{"access_token":"tok"}`
	})

	if _, err := FetchClientCredentials(context.Background(), srv.URL, "id", "secret", ""); err != nil {
		t.Fatal(err)
	}
	mu.Lock()
	entry := cache[cacheKey{srv.URL, "id", ""}]
	mu.Unlock()
	// An absent expires_in must not become a zero expiry, which would make the
	// cache useless and hammer the token endpoint once per request.
	if d := time.Until(entry.expires); d < 55*time.Minute || d > time.Hour {
		t.Errorf("default expiry was %v, want ~1h", d)
	}
}

func TestAnErrorResponseIsNotCached(t *testing.T) {
	resetCache(t)
	srv := newTokenServer(t, func(call int32) (int, string) {
		if call == 1 {
			return 401, `{"error":"invalid_client"}`
		}
		return 200, `{"access_token":"tok","expires_in":3600}`
	})

	_, err := FetchClientCredentials(context.Background(), srv.URL, "id", "secret", "")
	if err == nil {
		t.Fatal("a 401 from the token endpoint did not produce an error")
	}
	// The status and the body both belong in the message: "invalid_client" and
	// "invalid_scope" come back with the same status and want different fixes.
	if !strings.Contains(err.Error(), "401") || !strings.Contains(err.Error(), "invalid_client") {
		t.Errorf("error lost the endpoint's answer: %v", err)
	}

	// A failure must not poison the cache — the next attempt has to reach the
	// endpoint again.
	if _, err := FetchClientCredentials(context.Background(), srv.URL, "id", "secret", ""); err != nil {
		t.Fatalf("retry after a 401 failed: %v", err)
	}
}

func TestAResponseWithNoAccessTokenIsAnError(t *testing.T) {
	resetCache(t)
	srv := newTokenServer(t, func(int32) (int, string) {
		// A 200 that carries no token: the shape a misconfigured endpoint
		// returns, and the one that would otherwise send an empty bearer.
		return 200, `{"token_type":"Bearer","expires_in":3600}`
	})

	if _, err := FetchClientCredentials(context.Background(), srv.URL, "id", "secret", ""); err == nil {
		t.Error("a 200 with no access_token was accepted")
	}
}

func TestAnUnparseableBodyIsAnError(t *testing.T) {
	resetCache(t)
	srv := newTokenServer(t, func(int32) (int, string) {
		return 200, `<html>login required</html>`
	})

	_, err := FetchClientCredentials(context.Background(), srv.URL, "id", "secret", "")
	if err == nil {
		t.Fatal("an HTML body was accepted as a token response")
	}
	if !strings.Contains(err.Error(), "decode") {
		t.Errorf("error did not say what failed: %v", err)
	}
}

func TestMissingCredentialsFailBeforeAnyRequest(t *testing.T) {
	resetCache(t)
	srv := newTokenServer(t, func(int32) (int, string) {
		return 200, `{"access_token":"tok"}`
	})

	for _, tc := range []struct{ name, url, id string }{
		{"no token url", "", "id"},
		{"no client id", srv.URL, ""},
	} {
		if _, err := FetchClientCredentials(context.Background(), tc.url, tc.id, "secret", ""); err == nil {
			t.Errorf("%s: expected an error", tc.name)
		}
	}
	if n := srv.calls.Load(); n != 0 {
		t.Errorf("token endpoint was hit %d times, want 0", n)
	}
}

func TestACancelledContextStopsTheFetch(t *testing.T) {
	resetCache(t)
	srv := newTokenServer(t, func(int32) (int, string) {
		return 200, `{"access_token":"tok"}`
	})

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := FetchClientCredentials(ctx, srv.URL, "id", "secret", ""); err == nil {
		t.Error("a cancelled context still produced a token")
	}
}
