package app

import (
	"context"
	"errors"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/bgunnarsson/binman/internal/brufile"
	"github.com/bgunnarsson/binman/internal/curlfile"
	"github.com/bgunnarsson/binman/internal/envfile"
	"github.com/bgunnarsson/binman/internal/extract"
	"github.com/bgunnarsson/binman/internal/fsview"
	"github.com/bgunnarsson/binman/internal/graphqlfile"
	"github.com/bgunnarsson/binman/internal/history"
	"github.com/bgunnarsson/binman/internal/httpclient"
	"github.com/bgunnarsson/binman/internal/httpfile"
	"github.com/bgunnarsson/binman/internal/multipart"
	"github.com/bgunnarsson/binman/internal/oauth2"
	"github.com/bgunnarsson/binman/internal/openapi"
	"github.com/bgunnarsson/binman/internal/postmanfile"
	"github.com/bgunnarsson/binman/internal/ui/widgets"
)

var debugLog *log.Logger

func init() {
	f, err := os.OpenFile("/tmp/binreq-debug.log", os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0644)
	if err == nil {
		debugLog = log.New(f, "", log.Ltime|log.Lmicroseconds)
	}
}

func dbg(format string, args ...any) {
	if debugLog != nil {
		debugLog.Printf(format, args...)
	}
}

// LoadFile parses a .http, .bru, or .graphql file and updates the view.
func (a *App) LoadFile(path string) {
	var req *httpfile.Request
	var err error
	switch strings.ToLower(filepath.Ext(path)) {
	case ".bru":
		req, err = brufile.Load(path)
	case ".graphql":
		req, err = graphqlfile.Load(path)
	default:
		req, err = httpfile.Load(path)
	}
	if err != nil {
		a.View.RespBodyTv.SetText("[red]Failed to load file: " + err.Error() + "[-]")
		a.View.SetRespTab(0)
		return
	}
	a.State.CollectionVars = nil
	if strings.EqualFold(filepath.Ext(path), ".bru") {
		a.State.CollectionVars = brufile.CollectionVars(filepath.Dir(path), a.State.Root)
	}
	a.State.CurrentFile = path
	a.View.SetCurrentFile(path)
	a.loadRequest(req, filepath.Dir(path))
}

// LoadPostmanRequest loads a specific request from a Postman collection.
func (a *App) LoadPostmanRequest(collectionPath string, itemPath []int) {
	data, err := os.ReadFile(collectionPath)
	if err != nil {
		a.View.RespBodyTv.SetText("[red]Failed to read collection: " + err.Error() + "[-]")
		a.View.SetRespTab(0)
		return
	}
	c, err := postmanfile.Parse(data)
	if err != nil {
		a.View.RespBodyTv.SetText("[red]Failed to parse collection: " + err.Error() + "[-]")
		a.View.SetRespTab(0)
		return
	}
	req, err := postmanfile.RequestAt(c, itemPath)
	if err != nil {
		a.View.RespBodyTv.SetText("[red]Failed to extract request: " + err.Error() + "[-]")
		a.View.SetRespTab(0)
		return
	}
	a.State.CollectionVars = c.Vars()
	a.State.CurrentFile = collectionPath
	a.View.SetCurrentFile(collectionPath)
	a.loadRequest(req, filepath.Dir(collectionPath))
}

// LoadOpenAPIOperation loads a specific operation from an OpenAPI spec file.
func (a *App) LoadOpenAPIOperation(specPath, path, method string) {
	data, err := os.ReadFile(specPath)
	if err != nil {
		a.View.RespBodyTv.SetText("[red]Failed to read spec: " + err.Error() + "[-]")
		a.View.SetRespTab(0)
		return
	}
	spec, err := openapi.Parse(data, filepath.Base(specPath))
	if err != nil {
		a.View.RespBodyTv.SetText("[red]Failed to parse spec: " + err.Error() + "[-]")
		a.View.SetRespTab(0)
		return
	}
	req, err := openapi.OperationRequest(spec, path, method)
	if err != nil {
		a.View.RespBodyTv.SetText("[red]Failed to extract operation: " + err.Error() + "[-]")
		a.View.SetRespTab(0)
		return
	}
	a.State.CollectionVars = nil
	a.State.CurrentFile = specPath
	a.View.SetCurrentFile(specPath)
	a.loadRequest(req, filepath.Dir(specPath))
}

// loadRequest updates the view with a parsed request and discovers env sources in dir.
func (a *App) loadRequest(req *httpfile.Request, dir string) {
	a.State.CurrentRequest = req
	a.View.UpdateRequestView(req)

	sources := DiscoverEnvSources(dir, a.State.Root)
	dbg("loadRequest: dir=%s envSources=%d", dir, len(sources))
	for i, s := range sources {
		dbg("  env[%d]: kind=%d label=%s path=%s", i, s.Kind, s.Label, s.Path)
	}
	a.State.EnvFiles = sources
	labels := make([]string, len(sources))
	for i, s := range sources {
		labels[i] = s.Label
	}
	a.View.SetEnvOptions(labels)

	a.populateVarsTab()
}

// populateVarsTab scans the live request fields (URL bar, headers table, body)
// for {{var}} references and pre-fills the Vars tab with their currently-
// resolved values. Reads from the UI rather than State.CurrentRequest because
// the latter gets replaced with the resolved snapshot after a send.
func (a *App) populateVarsTab() {
	var combined strings.Builder
	combined.WriteString(a.View.URLInput.GetText())
	combined.WriteByte('\n')
	for k, v := range a.View.GetHeaders() {
		combined.WriteString(k)
		combined.WriteByte(':')
		combined.WriteString(v)
		combined.WriteByte('\n')
	}
	combined.WriteString(a.View.GetBody())

	names := envfile.Scan(combined.String())
	if len(names) == 0 {
		a.View.SetVars(nil)
		return
	}

	base := a.resolveBaseVars()
	pairs := make([]widgets.KVPair, 0, len(names))
	for _, name := range names {
		pairs = append(pairs, widgets.KVPair{Key: name, Value: base[name]})
	}
	a.View.SetVars(pairs)
}

// SendRequest executes the current request in a goroutine and updates the view.
func (a *App) SendRequest() {
	if a.State.Sending {
		return
	}

	// Build request from current UI state
	_, method := a.View.Method.GetCurrentOption()
	url := a.View.URLInput.GetText()
	if url == "" {
		return
	}

	// Resolve env variables
	vars := a.resolveEnvVars()

	req := &httpfile.Request{
		Method:  method,
		URL:     envfile.Resolve(url, vars),
		Headers: map[string]string{},
	}

	// Headers and auth come from their live KVTables so user edits are always picked up.
	for k, v := range a.View.GetHeaders() {
		req.Headers[k] = envfile.Resolve(v, vars)
	}

	ctx, cancel := context.WithCancel(context.Background())
	a.State.Cancel = cancel

	// Auth tab — Basic/Bearer/API-Key are returned as headers; OAuth2 client
	// credentials needs an out-of-band token fetch first.
	authType, authPairs := a.View.GetAuthRaw()
	if authType == "OAuth2 Client Credentials" {
		tokenURL := envfile.Resolve(authPairs["Token URL"], vars)
		clientID := envfile.Resolve(authPairs["Client ID"], vars)
		clientSecret := envfile.Resolve(authPairs["Client Secret"], vars)
		scope := envfile.Resolve(authPairs["Scope"], vars)
		token, err := oauth2.FetchClientCredentials(ctx, tokenURL, clientID, clientSecret, scope)
		if err != nil {
			a.State.Cancel = nil
			a.View.RespBodyTv.SetText(fmt.Sprintf("[red]OAuth2 token fetch failed: %v[-]", err))
			a.View.SetRespTab(0)
			return
		}
		req.Headers["Authorization"] = "Bearer " + token
	} else {
		for k, v := range a.View.GetAuth() {
			if k != "" {
				req.Headers[k] = envfile.Resolve(v, vars)
			}
		}
	}

	// Body comes from the active body type widget.
	bodyType := a.View.ReqBodyType
	if bodyType == "Multipart Form" {
		fields := []multipart.Field{}
		for _, p := range a.View.GetFormPairs() {
			fields = append(fields, multipart.Field{
				Name:  p.Key,
				Value: envfile.Resolve(p.Value, vars),
			})
		}
		body, ct, err := multipart.Encode(fields)
		if err != nil {
			a.State.Cancel = nil
			a.View.RespBodyTv.SetText(fmt.Sprintf("[red]Multipart encode failed: %v[-]", err))
			a.View.SetRespTab(0)
			return
		}
		req.Body = string(body)
		req.Headers["Content-Type"] = ct
	} else {
		req.Body = envfile.Resolve(a.View.GetBody(), vars)
		// Auto-set Content-Type from body type unless the user already set it.
		if ct := a.View.GetBodyContentType(); ct != "" {
			if _, exists := req.Headers["Content-Type"]; !exists {
				req.Headers["Content-Type"] = ct
			}
		}
	}

	// Persist a snapshot of the request as it will go out, so write-back / curl
	// export can use the resolved-and-built version.
	a.State.CurrentRequest = req

	a.State.Sending = true
	a.View.UpdateStatus(true)

	scriptText := a.View.GetScripts()

	go func() {
		defer cancel()
		dbg("Execute start: %s %s", req.Method, req.URL)

		// Stream handler: append events live to the response body view.
		var streamHandler httpclient.StreamHandler
		streamHandler = func(event string) bool {
			a.TV.QueueUpdateDraw(func() {
				cur := a.View.RespBodyTv.GetText(true)
				a.View.RespBodyTv.SetText(cur + event)
			})
			return ctx.Err() == nil
		}

		t0 := time.Now()
		resp := httpclient.Execute(ctx, req, streamHandler)
		dbg("Execute done: %v, body=%d bytes, err=%v", time.Since(t0), len(resp.Body), resp.Err)

		// Apply extraction rules from Scripts tab.
		var extracted map[string]string
		if resp.Err == nil {
			rules := extract.ParseRules(scriptText)
			if len(rules) > 0 {
				extracted = extract.Apply(rules, resp.Body, resp.Headers)
			}
		}

		// Append to history (best-effort; ignored on error).
		history.Append(history.Entry{
			Timestamp: time.Now(),
			Method:    req.Method,
			URL:       req.URL,
			Headers:   req.Headers,
			Body:      req.Body,
			Status:    resp.StatusCode,
			Duration:  resp.Duration,
		})

		formatted := httpclient.HighlightBody(resp.Body)

		a.TV.QueueUpdateDraw(func() {
			a.State.Sending = false
			a.State.Cancel = nil
			a.State.LastResponse = resp
			if len(extracted) > 0 {
				if a.State.ExtractedVars == nil {
					a.State.ExtractedVars = map[string]string{}
				}
				for k, v := range extracted {
					a.State.ExtractedVars[k] = v
				}
			}

			if resp.Err != nil && errors.Is(resp.Err, context.Canceled) {
				a.View.RespBodyTv.SetText("[#a78bfa]Request cancelled[-]")
				a.View.SetRespTab(0)
				a.View.UpdateStatus(false)
				return
			}
			a.View.UpdateResponseView(resp, formatted)
			a.View.UpdateCookies(resp.Cookies, httpclient.CookiesFor(req.URL))
			a.View.UpdateTrace(resp.Trace)
			a.View.UpdateScriptsResult(extracted)
		})
	}()
}

// CancelRequest cancels the in-flight request, if any.
func (a *App) CancelRequest() bool {
	if a.State.Cancel == nil {
		return false
	}
	a.State.Cancel()
	return true
}

// SaveCurrentRequest writes the current in-memory request back to its source
// file. Only .http and .bru sources support write-back.
func (a *App) SaveCurrentRequest() error {
	if a.State.CurrentRequest == nil || a.State.CurrentFile == "" {
		return errors.New("nothing to save")
	}
	// Rebuild from live UI state so unsaved field edits get persisted too.
	req := a.buildRequestFromUI(false)
	switch strings.ToLower(filepath.Ext(a.State.CurrentFile)) {
	case ".http":
		return httpfile.Save(a.State.CurrentFile, req)
	case ".bru":
		return brufile.Save(a.State.CurrentFile, req)
	}
	return fmt.Errorf("save not supported for %s", filepath.Ext(a.State.CurrentFile))
}

// buildRequestFromUI snapshots the current UI fields into a Request without
// running env-var resolution (so the file stays as-typed).
// If resolve is true, {{VAR}} placeholders are replaced.
func (a *App) buildRequestFromUI(resolve bool) *httpfile.Request {
	_, method := a.View.Method.GetCurrentOption()
	url := a.View.URLInput.GetText()
	req := &httpfile.Request{
		Method:  method,
		URL:     url,
		Headers: map[string]string{},
		Body:    a.View.GetBody(),
	}
	for k, v := range a.View.GetHeaders() {
		req.Headers[k] = v
	}
	if ct := a.View.GetBodyContentType(); ct != "" {
		if _, exists := req.Headers["Content-Type"]; !exists {
			req.Headers["Content-Type"] = ct
		}
	}
	if resolve {
		vars := a.resolveEnvVars()
		req.URL = envfile.Resolve(req.URL, vars)
		req.Body = envfile.Resolve(req.Body, vars)
		for k, v := range req.Headers {
			req.Headers[k] = envfile.Resolve(v, vars)
		}
	}
	return req
}

// SaveResponseBody writes the last response body to path.
func (a *App) SaveResponseBody(path string) error {
	if a.State.LastResponse == nil {
		return errors.New("no response to save")
	}
	return os.WriteFile(path, []byte(a.State.LastResponse.Body), 0o644)
}

// resolveEnvVars returns the merged variable map for the current request.
// Precedence (lowest → highest):
//  1. Collection vars (Postman variable[] / Bruno collection.bru,folder.bru)
//  2. Selected env source (.env / Bruno environments/*.bru / *.postman_environment.json)
//  3. Request-level vars block (e.g. Bruno `vars:pre-request`)
//  4. Extracted vars (from previous response extractors)
//  5. User overrides from the Vars tab
func (a *App) resolveEnvVars() map[string]string {
	merged := a.resolveBaseVars()
	if merged == nil {
		merged = map[string]string{}
	}
	for k, v := range a.View.GetVars() {
		merged[k] = v
	}
	if len(merged) == 0 {
		return nil
	}
	return merged
}

// resolveBaseVars returns the resolved variable map without applying the user
// overrides from the Vars tab. Used to pre-fill that tab on file load.
func (a *App) resolveBaseVars() map[string]string {
	merged := map[string]string{}
	for k, v := range a.State.CollectionVars {
		merged[k] = v
	}

	idx := a.View.EnvSelectedIndex()
	dbg("resolveBaseVars: EnvIndex=%d EnvSources=%d", idx, len(a.State.EnvFiles))
	if idx >= 0 && idx < len(a.State.EnvFiles) {
		envVars, err := a.State.EnvFiles[idx].Parse()
		dbg("resolveBaseVars: parsed vars=%d err=%v", len(envVars), err)
		if err == nil {
			for k, v := range envVars {
				merged[k] = v
			}
		}
	}

	if a.State.CurrentRequest != nil {
		for k, v := range a.State.CurrentRequest.Vars {
			merged[k] = v
		}
	}

	for k, v := range a.State.ExtractedVars {
		merged[k] = v
	}

	if len(merged) == 0 {
		return nil
	}
	return merged
}

// PromptSaveResponse opens a modal asking for a file path, then writes the
// last response body there.
func (a *App) PromptSaveResponse() {
	if a.State.LastResponse == nil {
		return
	}
	suggested := filepath.Join(a.State.Root, "response.txt")
	a.View.PromptString("Save response to file", suggested, func(path string) {
		if path == "" {
			return
		}
		if err := a.SaveResponseBody(path); err != nil {
			a.View.UpdateStatusError(fmt.Sprintf("save failed: %v", err))
			return
		}
		a.View.UpdateStatusError(fmt.Sprintf("saved to %s", path))
	})
}

// PromptSaveRequest writes the current request back to its source file.
func (a *App) PromptSaveRequest() {
	if err := a.SaveCurrentRequest(); err != nil {
		a.View.UpdateStatusError(fmt.Sprintf("save failed: %v", err))
		return
	}
	a.View.UpdateStatusError(fmt.Sprintf("saved %s", a.State.CurrentFile))
}

// PromptCopyCurl prints the current request as a curl one-liner into the
// response body view (so the user can copy it with their terminal).
func (a *App) PromptCopyCurl() {
	req := a.buildRequestFromUI(true)
	cmd := curlfile.Format(req)
	a.View.RespBodyTv.SetText("[#a78bfa]curl command (select to copy):[-]\n\n" + cmd)
	a.View.SetRespTab(0)
}

// MaybeImportCurl checks whether the current URL field contains a curl
// command and, if so, parses it and updates the request panel. Returns true
// if the URL was a curl invocation.
func (a *App) MaybeImportCurl() bool {
	text := a.View.URLInput.GetText()
	if !curlfile.IsCurl(text) {
		return false
	}
	req, err := curlfile.Parse(text)
	if err != nil {
		a.View.UpdateStatusError(fmt.Sprintf("curl parse failed: %v", err))
		return true
	}
	a.State.CurrentRequest = req
	a.View.UpdateRequestView(req)
	return true
}

// PromptHistory shows the request log; selecting an entry replays it.
func (a *App) PromptHistory() {
	entries, err := history.Load(50)
	if err != nil || len(entries) == 0 {
		a.View.UpdateStatusError("history is empty")
		return
	}
	labels := make([]string, len(entries))
	for i := range entries {
		// Show most recent first
		e := entries[len(entries)-1-i]
		labels[i] = fmt.Sprintf("%s  %d  %s  %s",
			e.Timestamp.Format("15:04:05"),
			e.Status,
			e.Method,
			e.URL,
		)
	}
	a.View.PromptList("History (Enter to replay)", labels, func(idx int) {
		e := entries[len(entries)-1-idx]
		req := &httpfile.Request{
			Method:  e.Method,
			URL:     e.URL,
			Headers: e.Headers,
			Body:    e.Body,
		}
		a.State.CurrentRequest = req
		a.View.UpdateRequestView(req)
		a.SendRequest()
	})
}

// PromptSearch opens a fuzzy-search modal across all collections under root.
func (a *App) PromptSearch() {
	entries := fsview.IndexAll(a.State.Root)
	if len(entries) == 0 {
		a.View.UpdateStatusError("no requests indexed")
		return
	}
	labels := make([]string, len(entries))
	for i, e := range entries {
		labels[i] = e.Label
	}
	a.View.PromptList("Search requests", labels, func(idx int) {
		e := entries[idx]
		switch e.Kind {
		case fsview.NodePostmanRequest:
			a.LoadPostmanRequest(e.Path, e.ItemPath)
		case fsview.NodeOpenAPIOperation:
			a.LoadOpenAPIOperation(e.Path, e.OpPath, e.Method)
		default:
			a.LoadFile(e.Path)
		}
	})
}

// PromptEnvEditor opens the currently selected .env file in a simple editor
// modal. On accept, writes back to disk.
func (a *App) PromptEnvEditor() {
	idx := a.View.EnvSelectedIndex()
	if idx < 0 || idx >= len(a.State.EnvFiles) {
		a.View.UpdateStatusError("no env file selected")
		return
	}
	path := a.State.EnvFiles[idx].Path
	data, err := os.ReadFile(path)
	if err != nil {
		a.View.UpdateStatusError(fmt.Sprintf("read failed: %v", err))
		return
	}
	a.View.PromptTextEdit("Edit "+filepath.Base(path), string(data), func(updated string) {
		if err := os.WriteFile(path, []byte(updated), 0o644); err != nil {
			a.View.UpdateStatusError(fmt.Sprintf("write failed: %v", err))
			return
		}
		a.View.UpdateStatusError("env saved")
	})
}

// CycleMethod rotates through HTTP methods.
func (a *App) CycleMethod() {
	methods := []string{"GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"}
	idx, _ := a.View.Method.GetCurrentOption()
	next := (idx + 1) % len(methods)
	a.View.Method.SetCurrentOption(next)
}

