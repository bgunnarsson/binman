package app

import (
	"fmt"
	"os"
	"runtime/debug"
	"time"

	"github.com/rivo/tview"

	"github.com/bgunnarsson/binman/internal/fsview"
	"github.com/bgunnarsson/binman/internal/ui"
)

// App is the top-level application object.
type App struct {
	TV    *tview.Application
	View  *ui.View
	State *State
}

// New constructs the App for the given collections root directory.
func New(root string) (*App, error) {
	st, err := os.Stat(root)
	if err != nil {
		return nil, fmt.Errorf("collections root %q not found: %w", root, err)
	}
	if !st.IsDir() {
		return nil, fmt.Errorf("collections root %q is not a directory", root)
	}

	a := &App{
		TV:    tview.NewApplication(),
		State: &State{Root: root},
	}

	// We build the view after the tree, since the tree's handler references the view.
	var view *ui.View

	openHTTPFile := func(path string) {
		defer func() {
			if r := recover(); r != nil {
				if view != nil {
					view.RespBodyTv.SetText(fmt.Sprintf("[red]PANIC: %v\n%s[-]", r, debug.Stack()))
					view.SetRespTab(0)
					a.TV.ForceDraw()
				}
			}
		}()

		if view == nil {
			return
		}

		a.LoadFile(path)
		a.TV.ForceDraw()
	}

	openPostmanRequest := func(collectionPath string, itemPath []int) {
		defer func() {
			if r := recover(); r != nil {
				if view != nil {
					view.RespBodyTv.SetText(fmt.Sprintf("[red]PANIC: %v\n%s[-]", r, debug.Stack()))
					view.SetRespTab(0)
					a.TV.ForceDraw()
				}
			}
		}()

		if view == nil {
			return
		}

		a.LoadPostmanRequest(collectionPath, itemPath)
		a.TV.ForceDraw()
	}

	openOpenAPIOperation := func(specPath, path, method string) {
		defer func() {
			if r := recover(); r != nil {
				if view != nil {
					view.RespBodyTv.SetText(fmt.Sprintf("[red]PANIC: %v\n%s[-]", r, debug.Stack()))
					view.SetRespTab(0)
					a.TV.ForceDraw()
				}
			}
		}()

		if view == nil {
			return
		}

		a.LoadOpenAPIOperation(specPath, path, method)
		a.TV.ForceDraw()
	}

	tree := fsview.NewTree(root, fsview.Filter{ShowNonHTTP: false}, fsview.Handlers{
		OpenHTTPFile:         openHTTPFile,
		OpenPostmanRequest:   openPostmanRequest,
		OpenOpenAPIOperation: openOpenAPIOperation,
	})

	view = ui.NewView(a.TV, tree)
	a.View = view

	a.TV.EnableMouse(true)
	a.TV.SetRoot(view.Root, true)
	a.TV.SetFocus(view.Sidebar)

	a.setupKeymap()

	return a, nil
}

// Run starts the tview event loop.
func (a *App) Run() error {
	go func() {
		for {
			time.Sleep(500 * time.Millisecond)
			dbg("heartbeat")
		}
	}()
	return a.TV.Run()
}
