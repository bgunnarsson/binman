package widgets

import (
	"github.com/gdamore/tcell/v2"
	"github.com/rivo/tview"
)

// Colors hardcoded to match the app theme (avoids an import cycle with ui).
var (
	kvBg      = tcell.NewHexColor(0x0d0f1e)
	kvBgPanel = tcell.NewHexColor(0x0f1122)
	kvText    = tcell.NewHexColor(0xd4d8e8)
	kvTextDim = tcell.NewHexColor(0x6b7090)
	kvAccent  = tcell.NewHexColor(0xa78bfa)
)

// KVPair is a single key-value entry (query param, header, etc.)
type KVPair struct {
	Key   string
	Value string
}

// KVTable is an interactive key-value editor backed by a tview.Table.
type KVTable struct {
	app      *tview.Application
	root     *tview.Flex
	table    *tview.Table
	keyInput *tview.InputField
	valInput *tview.InputField

	pairs     []KVPair
	editIndex int // index into pairs for the row being edited; -1 = new row
	onChange  func([]KVPair)
}

// NewKVTable creates a new interactive key-value table editor.
func NewKVTable(app *tview.Application) *KVTable {
	kv := &KVTable{
		app:       app,
		editIndex: -1,
	}

	kv.table = tview.NewTable()
	kv.table.SetBackgroundColor(kvBg)
	kv.table.SetSelectable(true, false)
	kv.table.SetSelectedStyle(tcell.StyleDefault.
		Background(tcell.NewHexColor(0x2d1f6e)).
		Foreground(tcell.ColorWhite))
	kv.table.SetFixed(1, 0)

	kv.keyInput = tview.NewInputField()
	kv.keyInput.SetLabel(" Key   ")
	kv.keyInput.SetLabelColor(kvTextDim)
	kv.keyInput.SetFieldBackgroundColor(kvBgPanel)
	kv.keyInput.SetFieldTextColor(kvText)
	kv.keyInput.SetBackgroundColor(kvBg)
	kv.keyInput.SetPlaceholderTextColor(kvTextDim)

	kv.valInput = tview.NewInputField()
	kv.valInput.SetLabel(" Value ")
	kv.valInput.SetLabelColor(kvTextDim)
	kv.valInput.SetFieldBackgroundColor(kvBgPanel)
	kv.valInput.SetFieldTextColor(kvText)
	kv.valInput.SetBackgroundColor(kvBg)
	kv.valInput.SetPlaceholderTextColor(kvTextDim)

	editBar := tview.NewFlex().SetDirection(tview.FlexColumn)
	editBar.SetBackgroundColor(kvBg)
	editBar.AddItem(kv.keyInput, 0, 1, false)
	editBar.AddItem(kv.valInput, 0, 1, false)

	kv.root = tview.NewFlex().SetDirection(tview.FlexRow)
	kv.root.SetBackgroundColor(kvBg)
	kv.root.AddItem(kv.table, 0, 1, true)
	kv.root.AddItem(editBar, 1, 0, false)

	kv.setupTableKeys()
	kv.setupKeyInputKeys()
	kv.setupValInputKeys()
	kv.render()

	return kv
}

// Widget returns the root primitive for embedding in layouts.
func (kv *KVTable) Widget() tview.Primitive { return kv.root }

// ContainsFocus reports whether p is one of the KVTable's internal widgets.
func (kv *KVTable) ContainsFocus(p tview.Primitive) bool {
	return p == kv.root || p == kv.table || p == kv.keyInput || p == kv.valInput
}

// OnChange registers a callback fired after every add/edit/delete.
func (kv *KVTable) OnChange(fn func([]KVPair)) *KVTable {
	kv.onChange = fn
	return kv
}

// SetPairs replaces the current pairs. Does NOT fire onChange.
func (kv *KVTable) SetPairs(pairs []KVPair) {
	kv.pairs = make([]KVPair, len(pairs))
	copy(kv.pairs, pairs)
	kv.editIndex = -1
	kv.keyInput.SetText("")
	kv.valInput.SetText("")
	kv.render()
}

// GetPairs returns a copy of the current pairs.
func (kv *KVTable) GetPairs() []KVPair {
	out := make([]KVPair, len(kv.pairs))
	copy(out, kv.pairs)
	return out
}

func (kv *KVTable) render() {
	kv.table.Clear()

	kv.table.SetCell(0, 0, tview.NewTableCell(" KEY").
		SetTextColor(kvTextDim).SetSelectable(false).SetExpansion(1))
	kv.table.SetCell(0, 1, tview.NewTableCell("VALUE").
		SetTextColor(kvTextDim).SetSelectable(false).SetExpansion(2))

	for i, p := range kv.pairs {
		kv.table.SetCell(i+1, 0, tview.NewTableCell(" "+p.Key).
			SetTextColor(kvAccent).SetExpansion(1))
		kv.table.SetCell(i+1, 1, tview.NewTableCell(p.Value).
			SetTextColor(kvText).SetExpansion(2))
	}

	addRow := len(kv.pairs) + 1
	kv.table.SetCell(addRow, 0, tview.NewTableCell(" + new parameter").
		SetTextColor(kvTextDim).SetExpansion(1))
	kv.table.SetCell(addRow, 1, tview.NewTableCell("").
		SetTextColor(kvTextDim).SetExpansion(2))
}

func (kv *KVTable) setupTableKeys() {
	kv.table.SetInputCapture(func(ev *tcell.EventKey) *tcell.EventKey {
		row, _ := kv.table.GetSelection()
		pairIdx := row - 1 // row 0 is the header

		switch ev.Key() {
		case tcell.KeyEnter:
			if pairIdx >= 0 && pairIdx < len(kv.pairs) {
				kv.editIndex = pairIdx
				kv.keyInput.SetText(kv.pairs[pairIdx].Key)
				kv.valInput.SetText(kv.pairs[pairIdx].Value)
			} else {
				kv.editIndex = -1
				kv.keyInput.SetText("")
				kv.valInput.SetText("")
			}
			kv.app.SetFocus(kv.keyInput)
			return nil
		}

		if ev.Key() == tcell.KeyRune && ev.Rune() == 'd' {
			if pairIdx >= 0 && pairIdx < len(kv.pairs) {
				kv.pairs = append(kv.pairs[:pairIdx], kv.pairs[pairIdx+1:]...)
				kv.render()
				if row > len(kv.pairs) {
					row = len(kv.pairs)
				}
				if row < 1 {
					row = 1
				}
				kv.table.Select(row, 0)
				kv.fire()
			}
			return nil
		}

		return ev
	})
}

func (kv *KVTable) setupKeyInputKeys() {
	kv.keyInput.SetInputCapture(func(ev *tcell.EventKey) *tcell.EventKey {
		switch ev.Key() {
		case tcell.KeyTab, tcell.KeyEnter:
			kv.app.SetFocus(kv.valInput)
			return nil
		case tcell.KeyEscape:
			kv.app.SetFocus(kv.table)
			return nil
		}
		return ev
	})
}

func (kv *KVTable) setupValInputKeys() {
	kv.valInput.SetInputCapture(func(ev *tcell.EventKey) *tcell.EventKey {
		switch ev.Key() {
		case tcell.KeyEnter, tcell.KeyTab:
			key := kv.keyInput.GetText()
			val := kv.valInput.GetText()
			if key == "" && val == "" {
				kv.app.SetFocus(kv.table)
				return nil
			}
			if kv.editIndex >= 0 && kv.editIndex < len(kv.pairs) {
				kv.pairs[kv.editIndex] = KVPair{Key: key, Value: val}
			} else {
				kv.pairs = append(kv.pairs, KVPair{Key: key, Value: val})
				kv.editIndex = len(kv.pairs) - 1
			}
			selectRow := kv.editIndex + 1
			kv.render()
			kv.table.Select(selectRow, 0)
			kv.app.SetFocus(kv.table)
			kv.fire()
			return nil
		case tcell.KeyEscape:
			kv.app.SetFocus(kv.table)
			return nil
		}
		return ev
	})
}

func (kv *KVTable) fire() {
	if kv.onChange != nil {
		kv.onChange(kv.GetPairs())
	}
}
