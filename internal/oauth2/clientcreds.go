// Package oauth2 implements just enough of the OAuth 2.0 client credentials
// grant to fetch an access token before sending the actual request.
//
// Tokens are cached in memory keyed by (token_url, client_id, scope) and
// refreshed when expired.
package oauth2

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"sync"
	"time"
)

type cacheKey struct {
	tokenURL string
	clientID string
	scope    string
}

type cachedToken struct {
	token   string
	expires time.Time
}

var (
	mu    sync.Mutex
	cache = map[cacheKey]cachedToken{}
)

// FetchClientCredentials returns an access token for the given client. The
// token is cached and reused until ~30 seconds before expiry.
func FetchClientCredentials(ctx context.Context, tokenURL, clientID, clientSecret, scope string) (string, error) {
	if tokenURL == "" || clientID == "" {
		return "", fmt.Errorf("oauth2: token URL and client ID are required")
	}
	key := cacheKey{tokenURL, clientID, scope}

	mu.Lock()
	if t, ok := cache[key]; ok && time.Now().Before(t.expires.Add(-30*time.Second)) {
		mu.Unlock()
		return t.token, nil
	}
	mu.Unlock()

	form := url.Values{}
	form.Set("grant_type", "client_credentials")
	form.Set("client_id", clientID)
	form.Set("client_secret", clientSecret)
	if scope != "" {
		form.Set("scope", scope)
	}
	req, err := http.NewRequestWithContext(ctx, "POST", tokenURL, strings.NewReader(form.Encode()))
	if err != nil {
		return "", err
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	req.Header.Set("Accept", "application/json")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	if resp.StatusCode/100 != 2 {
		return "", fmt.Errorf("oauth2: token endpoint returned %s: %s", resp.Status, strings.TrimSpace(string(body)))
	}
	var tr struct {
		AccessToken string `json:"access_token"`
		ExpiresIn   int    `json:"expires_in"`
		TokenType   string `json:"token_type"`
	}
	if err := json.Unmarshal(body, &tr); err != nil {
		return "", fmt.Errorf("oauth2: decode token response: %w", err)
	}
	if tr.AccessToken == "" {
		return "", fmt.Errorf("oauth2: response had no access_token")
	}
	exp := time.Now().Add(1 * time.Hour) // sensible default if expires_in absent
	if tr.ExpiresIn > 0 {
		exp = time.Now().Add(time.Duration(tr.ExpiresIn) * time.Second)
	}
	mu.Lock()
	cache[key] = cachedToken{token: tr.AccessToken, expires: exp}
	mu.Unlock()
	return tr.AccessToken, nil
}
