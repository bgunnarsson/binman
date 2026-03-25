package app

import (
	"log"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/bgunnarsson/binman/internal/brufile"
	"github.com/bgunnarsson/binman/internal/envfile"
	"github.com/bgunnarsson/binman/internal/httpclient"
	"github.com/bgunnarsson/binman/internal/httpfile"
	"github.com/bgunnarsson/binman/internal/postmanfile"
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

// LoadFile parses a .http or .bru file and updates the view.
func (a *App) LoadFile(path string) {
	var req *httpfile.Request
	var err error
	switch strings.ToLower(filepath.Ext(path)) {
	case ".bru":
		req, err = brufile.Load(path)
	default:
		req, err = httpfile.Load(path)
	}
	if err != nil {
		a.View.RespBodyTv.SetText("[red]Failed to load file: " + err.Error() + "[-]")
		a.View.SetRespTab(0)
		return
	}
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
	a.loadRequest(req, filepath.Dir(collectionPath))
}

// loadRequest updates the view with a parsed request and discovers env files in dir.
func (a *App) loadRequest(req *httpfile.Request, dir string) {
	a.State.CurrentRequest = req
	a.View.UpdateRequestView(req)

	envFiles := envfile.Find(dir)
	dbg("loadRequest: dir=%s envFiles=%d", dir, len(envFiles))
	for i, ef := range envFiles {
		dbg("  env[%d]: label=%s path=%s", i, ef.Label, ef.Path)
	}
	a.State.EnvFiles = envFiles
	labels := make([]string, len(envFiles))
	for i, ef := range envFiles {
		labels[i] = ef.Label
	}
	a.View.SetEnvOptions(labels)
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

	if a.State.CurrentRequest != nil {
		for k, v := range a.State.CurrentRequest.Headers {
			req.Headers[k] = envfile.Resolve(v, vars)
		}
		req.Body = envfile.Resolve(a.State.CurrentRequest.Body, vars)
	}

	a.State.Sending = true
	a.View.UpdateStatus(true)

	go func() {
		dbg("Execute start: %s %s", req.Method, req.URL)
		t0 := time.Now()
		resp := httpclient.Execute(req)
		dbg("Execute done: %v, body=%d bytes, err=%v", time.Since(t0), len(resp.Body), resp.Err)

		dbg("FormatBody start")
		formatted := httpclient.HighlightBody(resp.Body)
		dbg("FormatBody done: %d bytes -> %d bytes", len(resp.Body), len(formatted))

		dbg("QueueUpdateDraw enqueue")
		a.TV.QueueUpdateDraw(func() {
			dbg("QueueUpdateDraw callback start")
			a.State.Sending = false
			a.State.LastResponse = resp
			dbg("UpdateResponseView start")
			a.View.UpdateResponseView(resp, formatted)
			dbg("UpdateResponseView done")
		})
		dbg("QueueUpdateDraw returned")
	}()
}

// resolveEnvVars parses the currently selected env file and returns its variables.
func (a *App) resolveEnvVars() map[string]string {
	idx := a.View.EnvSelectedIndex()
	dbg("resolveEnvVars: EnvIndex=%d EnvFiles=%d", idx, len(a.State.EnvFiles))
	if idx < 0 || idx >= len(a.State.EnvFiles) {
		return nil
	}
	vars, err := envfile.Parse(a.State.EnvFiles[idx].Path)
	dbg("resolveEnvVars: parsed vars=%d err=%v", len(vars), err)
	if err != nil {
		return nil
	}
	return vars
}


// CycleMethod rotates through HTTP methods.
func (a *App) CycleMethod() {
	methods := []string{"GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"}
	idx, _ := a.View.Method.GetCurrentOption()
	next := (idx + 1) % len(methods)
	a.View.Method.SetCurrentOption(next)
}
