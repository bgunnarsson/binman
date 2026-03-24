package app

import (
	"github.com/bgunnarsson/binreq/internal/httpclient"
	"github.com/bgunnarsson/binreq/internal/httpfile"
)

// State holds the runtime state of the app.
type State struct {
	CurrentFile    string
	CurrentRequest *httpfile.Request
	LastResponse   *httpclient.Response
	Sending        bool
}
