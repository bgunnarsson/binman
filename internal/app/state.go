package app

import (
	"context"

	"github.com/bgunnarsson/binman/internal/envfile"
	"github.com/bgunnarsson/binman/internal/httpclient"
	"github.com/bgunnarsson/binman/internal/httpfile"
)

// State holds the runtime state of the app.
type State struct {
	Root           string // HTTP_FILES root directory
	CurrentFile    string
	CurrentRequest *httpfile.Request
	LastResponse   *httpclient.Response
	Sending        bool
	Cancel         context.CancelFunc // cancels the in-flight request when set
	EnvFiles       []envfile.EnvFile  // env files found by walking up from the current file's directory
	CollectionVars map[string]string  // variables defined in the active Postman collection (if any)
	ExtractedVars  map[string]string  // values extracted from previous responses
}
