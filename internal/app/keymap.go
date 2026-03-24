package app

import (
	"github.com/gdamore/tcell/v2"
	"github.com/rivo/tview"
)

// setupKeymap installs global key handlers on the tview application.
func (a *App) setupKeymap() {
	a.TV.SetInputCapture(func(event *tcell.EventKey) *tcell.EventKey {
		dbg("key event: key=%v rune=%v", event.Key(), event.Rune())
		switch {
		case event.Key() == tcell.KeyCtrlC || event.Key() == tcell.KeyCtrlQ:
			a.TV.Stop()
			return nil

		case event.Key() == tcell.KeyCtrlJ:
			a.SendRequest()
			return nil

		case event.Key() == tcell.KeyCtrlT:
			a.CycleMethod()
			return nil

		case event.Key() == tcell.KeyEscape:
			// Return focus to sidebar
			a.TV.SetFocus(a.View.Sidebar)
			return nil

		case event.Key() == tcell.KeyTab:
			// Cycle: Sidebar → URLInput → SendBtn → ReqPanel → RespPanel → Sidebar
			focused := a.TV.GetFocus()
			var next tview.Primitive
			switch {
			case focused == a.View.Sidebar:
				next = a.View.URLInput
			case focused == a.View.URLInput:
				next = a.View.SendBtn
			case focused == a.View.SendBtn:
				next = a.View.ReqFocusWidget
			case a.View.IsInReqPanel(focused):
				next = a.View.RespFocusWidget
			default:
				next = a.View.Sidebar
			}
			a.TV.SetFocus(next)
			return nil
		}
		return event
	})

	// Send on button click
	a.View.SendBtn.SetSelectedFunc(func() {
		a.SendRequest()
	})

	// Send on Enter in URL input
	a.View.URLInput.SetDoneFunc(func(key tcell.Key) {
		if key == tcell.KeyEnter {
			a.SendRequest()
		}
	})
}

// focusOrder returns the next primitive in the focus cycle.
func focusOrder(current tview.Primitive, order []tview.Primitive) tview.Primitive {
	for i, p := range order {
		if p == current {
			return order[(i+1)%len(order)]
		}
	}
	return order[0]
}
