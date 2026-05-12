package app

import (
	"context"

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
	EnvFiles       []EnvSource        // env sources (dotenv, Bruno, Postman) for the current request
	CollectionVars map[string]string  // variables defined in the active collection (Postman variable[], Bruno collection.bru/folder.bru)
	ExtractedVars  map[string]string  // values extracted from previous responses
}
