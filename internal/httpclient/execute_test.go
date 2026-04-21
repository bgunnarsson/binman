package httpclient

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/bgunnarsson/binman/internal/httpfile"
)

func TestExecuteBasic(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(201)
		fmt.Fprint(w, `{"ok":true}`)
	}))
	defer srv.Close()

	ctx := context.Background()
	resp := Execute(ctx, &httpfile.Request{Method: "GET", URL: srv.URL}, nil)
	if resp.Err != nil {
		t.Fatal(resp.Err)
	}
	if resp.StatusCode != 201 {
		t.Errorf("status: %d", resp.StatusCode)
	}
	if !strings.Contains(resp.Body, "ok") {
		t.Errorf("body: %q", resp.Body)
	}
	if resp.Trace.Total == 0 {
		t.Error("trace.Total was zero")
	}
}

func TestExecuteCookieJar(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.SetCookie(w, &http.Cookie{Name: "session", Value: "abc"})
		w.WriteHeader(204)
	}))
	defer srv.Close()

	resp := Execute(context.Background(), &httpfile.Request{URL: srv.URL}, nil)
	if resp.Err != nil {
		t.Fatal(resp.Err)
	}
	if len(resp.Cookies) == 0 || resp.Cookies[0].Name != "session" {
		t.Errorf("cookies: %+v", resp.Cookies)
	}
	if got := CookiesFor(srv.URL); len(got) == 0 || got[0].Value != "abc" {
		t.Errorf("jar cookies: %+v", got)
	}
}

func TestExecuteCancellation(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		time.Sleep(2 * time.Second)
		w.WriteHeader(200)
	}))
	defer srv.Close()

	ctx, cancel := context.WithCancel(context.Background())
	go func() {
		time.Sleep(50 * time.Millisecond)
		cancel()
	}()
	resp := Execute(ctx, &httpfile.Request{URL: srv.URL}, nil)
	if resp.Err == nil {
		t.Fatal("expected cancellation error")
	}
	if !errors.Is(resp.Err, context.Canceled) {
		t.Errorf("expected context.Canceled, got %v", resp.Err)
	}
}

func TestExecuteSSEStream(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(200)
		fmt.Fprint(w, "data: one\n\ndata: two\n\n")
		if f, ok := w.(http.Flusher); ok {
			f.Flush()
		}
	}))
	defer srv.Close()

	var got []string
	resp := Execute(context.Background(), &httpfile.Request{URL: srv.URL}, func(ev string) bool {
		got = append(got, ev)
		return true
	})
	if resp.Err != nil {
		t.Fatal(resp.Err)
	}
	if !resp.Stream {
		t.Error("expected Stream=true")
	}
	if len(got) < 2 {
		t.Errorf("expected ≥2 events, got %d: %+v", len(got), got)
	}
}
