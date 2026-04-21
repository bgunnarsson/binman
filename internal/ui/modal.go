package ui

import (
	"github.com/gdamore/tcell/v2"
	"github.com/rivo/tview"
)

// PromptString shows a centered modal input field, calls onAccept with the
// entered text on Enter, and dismisses the modal on Escape (or after onAccept).
// The modal is overlaid via Pages and removed afterwards.
func (v *View) PromptString(title, initial string, onAccept func(string)) {
	app := v.app()
	if app == nil {
		return
	}

	input := tview.NewInputField()
	input.SetLabel(" ")
	input.SetText(initial)
	input.SetFieldBackgroundColor(ColorBgPanel)
	input.SetFieldTextColor(ColorText)
	input.SetBackgroundColor(ColorBgPanel)
	input.SetFieldWidth(0)

	frame := tview.NewFrame(input).
		SetBorders(1, 1, 0, 0, 1, 1)
	frame.SetBackgroundColor(ColorBgPanel)
	frame.SetBorder(true)
	frame.SetBorderColor(ColorAccentFg)
	frame.SetTitle(" " + title + " ")
	frame.SetTitleAlign(tview.AlignLeft)

	overlay := centered(frame, 60, 5)

	pages := v.modalPages()
	if pages == nil {
		return
	}
	pages.AddPage("modal", overlay, true, true)
	app.SetFocus(input)

	close := func() {
		pages.RemovePage("modal")
		app.SetFocus(v.URLInput)
	}

	input.SetDoneFunc(func(key tcell.Key) {
		switch key {
		case tcell.KeyEnter:
			text := input.GetText()
			close()
			if onAccept != nil {
				onAccept(text)
			}
		case tcell.KeyEscape:
			close()
		}
	})
}

// PromptList shows a centered modal listing items; onAccept is called with the
// chosen index. Escape dismisses.
func (v *View) PromptList(title string, items []string, onAccept func(int)) {
	app := v.app()
	if app == nil {
		return
	}
	if len(items) == 0 {
		v.PromptString(title+" (empty)", "", func(string) {})
		return
	}
	list := tview.NewList()
	list.SetBackgroundColor(ColorBgPanel)
	list.SetMainTextColor(ColorText)
	list.SetSelectedBackgroundColor(tcell.NewHexColor(0x2d1f6e))
	list.SetSelectedTextColor(tcell.ColorWhite)
	list.ShowSecondaryText(false)
	for i, it := range items {
		idx := i
		list.AddItem(it, "", 0, func() {
			pages := v.modalPages()
			if pages != nil {
				pages.RemovePage("modal")
			}
			app.SetFocus(v.URLInput)
			if onAccept != nil {
				onAccept(idx)
			}
		})
	}
	list.SetInputCapture(func(ev *tcell.EventKey) *tcell.EventKey {
		if ev.Key() == tcell.KeyEscape {
			pages := v.modalPages()
			if pages != nil {
				pages.RemovePage("modal")
			}
			app.SetFocus(v.URLInput)
			return nil
		}
		return ev
	})

	frame := tview.NewFrame(list).SetBorders(0, 0, 0, 0, 1, 1)
	frame.SetBackgroundColor(ColorBgPanel)
	frame.SetBorder(true)
	frame.SetBorderColor(ColorAccentFg)
	frame.SetTitle(" " + title + " ")
	frame.SetTitleAlign(tview.AlignLeft)

	height := len(items) + 4
	if height > 25 {
		height = 25
	}
	overlay := centered(frame, 80, height)

	pages := v.modalPages()
	if pages == nil {
		return
	}
	pages.AddPage("modal", overlay, true, true)
	app.SetFocus(list)
}

// PromptTextEdit shows a TextArea modal seeded with `initial`. On Ctrl+S the
// updated text is passed to onAccept; Escape dismisses without saving.
func (v *View) PromptTextEdit(title, initial string, onAccept func(string)) {
	app := v.app()
	if app == nil {
		return
	}
	area := tview.NewTextArea()
	area.SetText(initial, true)
	area.SetBackgroundColor(ColorBgPanel)
	area.SetTextStyle(tcell.StyleDefault.Background(ColorBgPanel).Foreground(ColorText))

	frame := tview.NewFrame(area).SetBorders(0, 0, 0, 0, 1, 1)
	frame.SetBackgroundColor(ColorBgPanel)
	frame.SetBorder(true)
	frame.SetBorderColor(ColorAccentFg)
	frame.SetTitle(" " + title + " — ^s save · esc cancel ")
	frame.SetTitleAlign(tview.AlignLeft)

	overlay := centered(frame, 80, 20)
	pages := v.modalPages()
	if pages == nil {
		return
	}
	pages.AddPage("modal", overlay, true, true)
	app.SetFocus(area)

	close := func() {
		pages.RemovePage("modal")
		app.SetFocus(v.URLInput)
	}

	area.SetInputCapture(func(ev *tcell.EventKey) *tcell.EventKey {
		switch ev.Key() {
		case tcell.KeyCtrlS:
			text := area.GetText()
			close()
			if onAccept != nil {
				onAccept(text)
			}
			return nil
		case tcell.KeyEscape:
			close()
			return nil
		}
		return ev
	})
}

// modalPages returns the top-level Pages container used for overlays.
func (v *View) modalPages() *tview.Pages { return v.Root }

// app returns the parent application reference stashed on the View.
func (v *View) app() *tview.Application { return v.appRef }

func centered(p tview.Primitive, width, height int) tview.Primitive {
	return tview.NewFlex().SetDirection(tview.FlexRow).
		AddItem(nil, 0, 1, false).
		AddItem(tview.NewFlex().SetDirection(tview.FlexColumn).
			AddItem(nil, 0, 1, false).
			AddItem(p, width, 0, true).
			AddItem(nil, 0, 1, false), height, 0, true).
		AddItem(nil, 0, 1, false)
}
