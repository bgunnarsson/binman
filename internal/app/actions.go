package app

import (
	"log"
	"os"
	"time"

	"github.com/bgunnarsson/binreq/internal/httpclient"
	"github.com/bgunnarsson/binreq/internal/httpfile"
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

// LoadFile parses a .http file and updates the view.
func (a *App) LoadFile(path string) {
	req, err := httpfile.Load(path)
	if err != nil {
		a.View.RespBodyTv.SetText("[red]Failed to load file: " + err.Error() + "[-]")
		a.View.SetRespTab(0)
		return
	}
	a.State.CurrentFile = path
	a.State.CurrentRequest = req
	a.View.SetCurrentFile(path)
	a.View.UpdateRequestView(req)
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

	req := &httpfile.Request{
		Method:  method,
		URL:     url,
		Headers: map[string]string{},
	}

	if a.State.CurrentRequest != nil {
		req.Headers = a.State.CurrentRequest.Headers
		req.Body = a.State.CurrentRequest.Body
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

// CycleMethod rotates through HTTP methods.
func (a *App) CycleMethod() {
	methods := []string{"GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"}
	idx, _ := a.View.Method.GetCurrentOption()
	next := (idx + 1) % len(methods)
	a.View.Method.SetCurrentOption(next)
}
