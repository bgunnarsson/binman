package app

import (
	"github.com/bgunnarsson/binman/internal/envfile"
	"github.com/bgunnarsson/binman/internal/httpclient"
	"github.com/bgunnarsson/binman/internal/httpfile"
)

// State holds the runtime state of the app.
type State struct {
	CurrentFile    string
	CurrentRequest *httpfile.Request
	LastResponse   *httpclient.Response
	Sending        bool
	EnvFiles []envfile.EnvFile // env files found in the current file's directory
}
