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
		case event.Key() == tcell.KeyCtrlC:
			// While a request is in flight, Ctrl+C cancels it instead of quitting.
			if a.CancelRequest() {
				return nil
			}
			a.TV.Stop()
			return nil

		case event.Key() == tcell.KeyCtrlQ:
			a.TV.Stop()
			return nil

		case event.Key() == tcell.KeyCtrlJ:
			a.SendRequest()
			return nil

		case event.Key() == tcell.KeyCtrlT:
			a.CycleMethod()
			return nil

		case event.Key() == tcell.KeyCtrlS:
			// Save: response if focused on response body, else write request to source.
			focused := a.TV.GetFocus()
			if focused == a.View.RespBodyTv {
				a.PromptSaveResponse()
			} else {
				a.PromptSaveRequest()
			}
			return nil

		case event.Key() == tcell.KeyCtrlY:
			a.PromptCopyCurl()
			return nil

		case event.Key() == tcell.KeyCtrlH:
			a.PromptHistory()
			return nil

		case event.Key() == tcell.KeyCtrlF:
			a.PromptSearch()
			return nil

		case event.Key() == tcell.KeyCtrlE:
			a.PromptEnvEditor()
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

		case event.Key() == tcell.KeyRune && event.Rune() == '[':
			focused := a.TV.GetFocus()
			if a.View.IsInReqPanelNav(focused) {
				a.View.SetReqTab(a.View.ReqActiveTab - 1)
				a.TV.SetFocus(a.View.ReqFocusWidget)
				return nil
			}
			if a.View.IsInRespPanel(focused) {
				a.View.SetRespTab(a.View.RespActiveTab - 1)
				a.TV.SetFocus(a.View.RespFocusWidget)
				return nil
			}

		case event.Key() == tcell.KeyRune && event.Rune() == ']':
			focused := a.TV.GetFocus()
			if a.View.IsInReqPanelNav(focused) {
				a.View.SetReqTab(a.View.ReqActiveTab + 1)
				a.TV.SetFocus(a.View.ReqFocusWidget)
				return nil
			}
			if a.View.IsInRespPanel(focused) {
				a.View.SetRespTab(a.View.RespActiveTab + 1)
				a.TV.SetFocus(a.View.RespFocusWidget)
				return nil
			}
		}
		return event
	})

	// Send on button click
	a.View.SendBtn.SetSelectedFunc(func() {
		a.SendRequest()
	})

	// Send on Enter in URL input — first see if it's a curl paste to import.
	a.View.URLInput.SetDoneFunc(func(key tcell.Key) {
		if key == tcell.KeyEnter {
			if a.MaybeImportCurl() {
				return
			}
			a.SendRequest()
		}
	})
}

