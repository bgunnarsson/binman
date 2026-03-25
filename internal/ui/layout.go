package ui

import (
	"fmt"
	"log"
	"net/url"
	"os"
	"strings"
	"github.com/gdamore/tcell/v2"
	"github.com/rivo/tview"

	"github.com/bgunnarsson/binman/internal/httpclient"
	"github.com/bgunnarsson/binman/internal/httpfile"
	"github.com/bgunnarsson/binman/internal/ui/widgets"
)

var uiLog *log.Logger

func init() {
	f, err := os.OpenFile("/tmp/binreq-debug.log", os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0644)
	if err == nil {
		uiLog = log.New(f, "[ui] ", log.Ltime|log.Lmicroseconds)
	}
}

func uiDbg(format string, args ...any) {
	if uiLog != nil {
		uiLog.Printf(format, args...)
	}
}

var (
	reqTabNames  = []string{"Params", "Headers", "Body", "Auth", "Info", "Scripts", "Options"}
	respTabNames = []string{"Body", "Headers", "Cookies", "Scripts", "Trace"}
)

// View holds all tview primitives that make up the UI.
type View struct {
	Root *tview.Flex

	// URL bar
	Method      *tview.DropDown
	EnvDropDown *tview.DropDown
	URLInput    *tview.InputField
	RespStatusCode *tview.TextView
	RespStatusBar  *tview.TextView
	SendBtn        *tview.Button

	// Focus helpers
	ReqFocusWidget  tview.Primitive // primary focusable widget in request panel
	RespFocusWidget tview.Primitive // primary focusable widget in response panel

	// Sidebar (populated by fsview.NewTree)
	Sidebar *tview.TreeView

	// Request panel
	ReqTabBar       *tview.TextView
	ReqTabUnderline *tview.TextView
	ReqPages        *tview.Pages
	ReqHeadersTv    *tview.TextView // "no headers" / headers display
	ReqBodyTv       *tview.TextView
	ReqParamsTable  *widgets.KVTable

	// Response panel
	RespTabBar       *tview.TextView
	RespTabUnderline *tview.TextView
	RespPages        *tview.Pages
	RespBodyTv       *tview.TextView
	RespHeadersTv    *tview.TextView

	// Status bar
	StatusBar *tview.TextView

	// Current file path shown in status bar
	CurrentFile string

	// Tab state
	ReqActiveTab  int
	RespActiveTab int
}

// NewView constructs the full layout around the provided sidebar tree.
func NewView(app *tview.Application, sidebar *tview.TreeView) *View {
	ApplyTheme()
	v := &View{Sidebar: sidebar}

	// --- Info bar ---
	appLabel := tview.NewTextView()
	appLabel.SetDynamicColors(true)
	appLabel.SetBackgroundColor(ColorBg)
	appLabel.SetText("[#a78bfa]BINMAN[-] [#4a4f72]0.0.1[-]")

	envNoStyle := tcell.StyleDefault.Background(tcell.NewHexColor(0x1e293b)).Foreground(tcell.NewHexColor(0x64748b))

	v.EnvDropDown = tview.NewDropDown()
	v.EnvDropDown.SetBackgroundColor(ColorBg)
	v.EnvDropDown.SetTextOptions(" ", " ", " ", "", "")
	v.EnvDropDown.SetListStyles(
		tcell.StyleDefault.Background(ColorBgPanel).Foreground(ColorText),
		tcell.StyleDefault.Background(tcell.NewHexColor(0x0d6b5e)).Foreground(tcell.ColorWhite),
	)
	v.EnvDropDown.SetOptions([]string{"no env"}, func(_ string, _ int) {
		v.EnvDropDown.SetFieldStyle(envNoStyle)
		v.EnvDropDown.SetFocusedStyle(envNoStyle)
	})
	v.EnvDropDown.SetFieldWidth(16)
	v.EnvDropDown.SetCurrentOption(0)

	infoBar := tview.NewFlex().SetDirection(tview.FlexColumn)
	infoBar.SetBackgroundColor(ColorBg)
	infoBar.AddItem(tview.NewBox().SetBackgroundColor(ColorBg), 2, 0, false)
	infoBar.AddItem(appLabel, 0, 1, false)
	infoBar.AddItem(v.EnvDropDown, 18, 0, false)
	infoBar.AddItem(tview.NewBox().SetBackgroundColor(ColorBg), 2, 0, false)


	infoBarWrapper := tview.NewFlex().SetDirection(tview.FlexRow)
	infoBarWrapper.SetBackgroundColor(ColorBg)
	infoBarWrapper.AddItem(tview.NewBox().SetBackgroundColor(ColorBg), 1, 0, false)
	infoBarWrapper.AddItem(infoBar, 1, 0, false)

	// --- URL bar ---
	methodStyles := map[string]tcell.Style{
		"GET":     tcell.StyleDefault.Background(tcell.NewHexColor(0x14532d)).Foreground(tcell.NewHexColor(0x86efac)),
		"POST":    tcell.StyleDefault.Background(tcell.NewHexColor(0x1e3a5f)).Foreground(tcell.NewHexColor(0x93c5fd)),
		"PUT":     tcell.StyleDefault.Background(tcell.NewHexColor(0x78350f)).Foreground(tcell.NewHexColor(0xfcd34d)),
		"PATCH":   tcell.StyleDefault.Background(tcell.NewHexColor(0x164e63)).Foreground(tcell.NewHexColor(0x67e8f9)),
		"DELETE":  tcell.StyleDefault.Background(tcell.NewHexColor(0x7f1d1d)).Foreground(tcell.NewHexColor(0xfca5a5)),
		"HEAD":    tcell.StyleDefault.Background(tcell.NewHexColor(0x3b0764)).Foreground(tcell.NewHexColor(0xe9d5ff)),
		"OPTIONS": tcell.StyleDefault.Background(tcell.NewHexColor(0x1e293b)).Foreground(tcell.NewHexColor(0x94a3b8)),
	}

	v.Method = tview.NewDropDown()
	v.Method.SetBackgroundColor(ColorBgPanel)
	v.Method.SetTextOptions(" ", " ", " ", "", "")
	v.Method.SetListStyles(
		tcell.StyleDefault.Background(ColorBgPanel).Foreground(ColorText),
		tcell.StyleDefault.Background(tcell.NewHexColor(0x5b21b6)).Foreground(tcell.ColorWhite),
	)
	v.Method.SetOptions([]string{"GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"}, func(text string, _ int) {
		if s, ok := methodStyles[text]; ok {
			v.Method.SetFieldStyle(s)
			v.Method.SetFocusedStyle(s)
		}
	})
	v.Method.SetCurrentOption(0)

	v.URLInput = tview.NewInputField()
	v.URLInput.SetPlaceholder("Enter a URL or paste a curl command...")
	v.URLInput.SetPlaceholderTextColor(ColorTextDim)
	v.URLInput.SetFieldBackgroundColor(ColorBgPanel)
	v.URLInput.SetFieldTextColor(ColorText)
	v.URLInput.SetBackgroundColor(ColorBgPanel)

	v.RespStatusCode = tview.NewTextView()
	v.RespStatusCode.SetDynamicColors(true)
	v.RespStatusCode.SetBackgroundColor(ColorBgPanel)
	v.RespStatusCode.SetTextAlign(tview.AlignRight)

	v.RespStatusBar = tview.NewTextView()
	v.RespStatusBar.SetDynamicColors(true)
	v.RespStatusBar.SetBackgroundColor(ColorBgPanel)

	v.SendBtn = tview.NewButton("Send")
	v.SendBtn.SetStyle(tcell.StyleDefault.Background(ColorSendBg).Foreground(ColorSendFg))
	v.SendBtn.SetActivatedStyle(tcell.StyleDefault.Background(ColorAccentFg).Foreground(ColorBg))

	urlBar := tview.NewFlex().SetDirection(tview.FlexColumn)
	urlBar.SetBackgroundColor(ColorBgPanel)
	urlBar.AddItem(v.Method, 10, 0, false)
	urlBar.AddItem(v.URLInput, 0, 1, false)
	urlBar.AddItem(v.RespStatusCode, 6, 0, false)
	urlBar.AddItem(v.RespStatusBar, 4, 0, false)
	urlBar.AddItem(v.SendBtn, 8, 0, false)

	urlBarRow := tview.NewFlex().SetDirection(tview.FlexColumn)
	urlBarRow.SetBackgroundColor(ColorBg)
	urlBarRow.AddItem(tview.NewBox().SetBackgroundColor(ColorBg), 2, 0, false)
	urlBarRow.AddItem(urlBar, 0, 1, false)
	urlBarRow.AddItem(tview.NewBox().SetBackgroundColor(ColorBg), 2, 0, false)

	urlBarWrapper := tview.NewFlex().SetDirection(tview.FlexRow)
	urlBarWrapper.SetBackgroundColor(ColorBg)
	urlBarWrapper.AddItem(tview.NewBox().SetBackgroundColor(ColorBg), 1, 0, false)
	urlBarWrapper.AddItem(urlBarRow, 1, 0, false)
	urlBarWrapper.AddItem(tview.NewBox().SetBackgroundColor(ColorBg), 1, 0, false)

	// --- Sidebar ---
	v.Sidebar.SetBorder(true)
	v.Sidebar.SetBorderColor(ColorBorder)
	v.Sidebar.SetTitleColor(ColorTextDim) // #6b7090 — visible but not bright
	v.Sidebar.SetTitle(" Collection ")
	v.Sidebar.SetTitleAlign(tview.AlignRight)
	v.Sidebar.SetBackgroundColor(ColorBg)
	v.Sidebar.SetGraphicsColor(ColorBorder)

	// --- Request panel ---
	v.ReqTabBar = tview.NewTextView()
	v.ReqTabBar.SetDynamicColors(true)
	v.ReqTabBar.SetBackgroundColor(ColorBg)

	v.ReqTabUnderline = tview.NewTextView()
	v.ReqTabUnderline.SetDynamicColors(true)
	v.ReqTabUnderline.SetBackgroundColor(ColorBg)

	v.ReqHeadersTv = tview.NewTextView()
	v.ReqHeadersTv.SetDynamicColors(true)
	v.ReqHeadersTv.SetBackgroundColor(ColorBg)
	v.ReqHeadersTv.SetTextAlign(tview.AlignCenter)
	v.ReqHeadersTv.SetText("\n\n\n[#4a4f72]No headers[-]")

	v.ReqBodyTv = tview.NewTextView()
	v.ReqBodyTv.SetDynamicColors(true)
	v.ReqBodyTv.SetBackgroundColor(ColorBg)
	v.ReqBodyTv.SetWrap(true)

	v.ReqParamsTable = widgets.NewKVTable(app)
	v.ReqParamsTable.OnChange(func(pairs []widgets.KVPair) {
		raw := v.URLInput.GetText()
		base := raw
		if idx := strings.Index(raw, "?"); idx >= 0 {
			base = raw[:idx]
		}
		q := url.Values{}
		for _, p := range pairs {
			if p.Key != "" {
				q.Add(p.Key, p.Value)
			}
		}
		if len(q) > 0 {
			v.URLInput.SetText(base + "?" + q.Encode())
		} else {
			v.URLInput.SetText(base)
		}
	})

	v.ReqPages = tview.NewPages()
	v.ReqPages.AddPage("Params", v.ReqParamsTable.Widget(), true, true)
	v.ReqPages.AddPage("Headers", v.ReqHeadersTv, true, false)
	v.ReqPages.AddPage("Body", v.ReqBodyTv, true, false)
	for _, name := range []string{"Auth", "Info", "Scripts", "Options"} {
		stub := tview.NewTextView()
		stub.SetDynamicColors(true)
		stub.SetBackgroundColor(ColorBg)
		stub.SetTextAlign(tview.AlignCenter)
		stub.SetText(fmt.Sprintf("\n\n\n[#4a4f72]%s[-]", name))
		v.ReqPages.AddPage(name, stub, true, false)
	}

	reqTabRow := tview.NewFlex().SetDirection(tview.FlexColumn)
	reqTabRow.SetBackgroundColor(ColorBg)
	reqTabRow.AddItem(v.ReqTabBar, 0, 1, false)

	reqPanel := tview.NewFlex().SetDirection(tview.FlexRow)
	reqPanel.SetBorder(true)
	reqPanel.SetBorderColor(ColorBorder)
	reqPanel.SetTitle(" Request ")
	reqPanel.SetTitleColor(ColorTextDim)
	reqPanel.SetTitleAlign(tview.AlignRight)
	reqPanel.SetBackgroundColor(ColorBg)
	reqPanel.AddItem(reqTabRow, 1, 0, false)
	reqPanel.AddItem(v.ReqTabUnderline, 1, 0, false)
	reqPanel.AddItem(v.ReqPages, 0, 1, false)

	// --- Response panel ---
	v.RespTabBar = tview.NewTextView()
	v.RespTabBar.SetDynamicColors(true)
	v.RespTabBar.SetBackgroundColor(ColorBg)

	v.RespTabUnderline = tview.NewTextView()
	v.RespTabUnderline.SetDynamicColors(true)
	v.RespTabUnderline.SetBackgroundColor(ColorBg)

	v.RespBodyTv = tview.NewTextView()
	v.RespBodyTv.SetDynamicColors(true)
	v.RespBodyTv.SetBackgroundColor(ColorBg)
	v.RespBodyTv.SetWrap(true)
	v.RespBodyTv.SetScrollable(true)
	v.RespBodyTv.SetBorderPadding(0, 0, 2, 2)

	v.RespHeadersTv = tview.NewTextView()
	v.RespHeadersTv.SetDynamicColors(true)
	v.RespHeadersTv.SetBackgroundColor(ColorBg)

	v.RespPages = tview.NewPages()
	v.RespPages.AddPage("Body", v.RespBodyTv, true, true)
	v.RespPages.AddPage("Headers", v.RespHeadersTv, true, false)
	for _, name := range []string{"Cookies", "Scripts", "Trace"} {
		stub := tview.NewTextView()
		stub.SetDynamicColors(true)
		stub.SetBackgroundColor(ColorBg)
		stub.SetTextAlign(tview.AlignCenter)
		stub.SetText(fmt.Sprintf("\n\n\n[#4a4f72]%s[-]", name))
		v.RespPages.AddPage(name, stub, true, false)
	}

	respTabRow := tview.NewFlex().SetDirection(tview.FlexColumn)
	respTabRow.SetBackgroundColor(ColorBg)
	respTabRow.AddItem(v.RespTabBar, 0, 1, false)

	respPanel := tview.NewFlex().SetDirection(tview.FlexRow)
	respPanel.SetBorder(true)
	respPanel.SetBorderColor(ColorBorder)
	respPanel.SetTitle(" Response ")
	respPanel.SetTitleColor(ColorTextDim)
	respPanel.SetTitleAlign(tview.AlignRight)
	respPanel.SetBackgroundColor(ColorBg)
	respPanel.AddItem(respTabRow, 1, 0, false)
	respPanel.AddItem(v.RespTabUnderline, 1, 0, false)
	respPanel.AddItem(v.RespPages, 0, 1, false)

	// --- Right panel (request + response stacked) ---
	rightPanel := tview.NewFlex().SetDirection(tview.FlexRow)
	rightPanel.SetBackgroundColor(ColorBg)
	rightPanel.AddItem(reqPanel, 0, 2, false)
	rightPanel.AddItem(respPanel, 0, 5, false)

	// --- Main row (sidebar + right) ---
	mainRow := tview.NewFlex().SetDirection(tview.FlexColumn)
	mainRow.SetBackgroundColor(ColorBg)
	mainRow.AddItem(v.Sidebar, 32, 0, true)
	mainRow.AddItem(rightPanel, 0, 1, false)

	// --- Status bar ---
	v.StatusBar = tview.NewTextView()
	v.StatusBar.SetDynamicColors(true)
	v.StatusBar.SetBackgroundColor(ColorStatusBg)
	v.StatusBar.SetText(statusBarText("", false))

	// --- Root ---
	v.Root = tview.NewFlex().SetDirection(tview.FlexRow)
	v.Root.SetBackgroundColor(ColorBg)
	v.Root.AddItem(infoBarWrapper, 2, 0, false)
	v.Root.AddItem(urlBarWrapper, 3, 0, false)
	v.Root.AddItem(mainRow, 0, 1, true)
	v.Root.AddItem(v.StatusBar, 1, 0, false)

	// Focus helpers
	v.ReqFocusWidget = v.ReqParamsTable.Widget()
	v.RespFocusWidget = v.RespBodyTv

	// Render initial tab bars
	v.renderReqTabBar()
	v.renderRespTabBar()

	// Wire tab click handlers
	v.wireTabClicks()

	return v
}

// renderReqTabBar re-renders the request tab names and underline rows.
func (v *View) renderReqTabBar() {
	v.ReqTabBar.SetText(" " + renderTabs(reqTabNames, v.ReqActiveTab))
	v.ReqTabUnderline.SetText(renderTabUnderline(reqTabNames, v.ReqActiveTab))
}

// renderRespTabBar re-renders the response tab names and underline rows.
func (v *View) renderRespTabBar() {
	v.RespTabBar.SetText(" " + renderTabs(respTabNames, v.RespActiveTab))
	v.RespTabUnderline.SetText(renderTabUnderline(respTabNames, v.RespActiveTab))
}

// renderTabs returns a tview-markup string of tab names; active tab is bright, others dim.
func renderTabs(tabs []string, active int) string {
	var b strings.Builder
	for i, tab := range tabs {
		if i == active {
			fmt.Fprintf(&b, "[#a78bfa]%s[-]", tab)
		} else {
			fmt.Fprintf(&b, "[#4a4f72]%s[-]", tab)
		}
		if i < len(tabs)-1 {
			b.WriteString("  ")
		}
	}
	return b.String()
}

// renderTabUnderline returns a tview-markup string with "─" under the active tab name.
func renderTabUnderline(tabs []string, active int) string {
	pos := 1 // mirrors the leading " " in the names row
	for i, tab := range tabs {
		if i == active {
			return strings.Repeat(" ", pos) + "[#a78bfa]" + strings.Repeat("─", len(tab)) + "[-]"
		}
		pos += len(tab) + 2 // 2-space separator
	}
	return ""
}

// tabIndexAtX returns the tab index for a given x position within the tab bar text.
// x=0 is the first character (which is a leading space).
func tabIndexAtX(tabs []string, x int) int {
	pos := 1 // leading space
	for i, tab := range tabs {
		end := pos + len(tab)
		if x >= pos && x < end {
			return i
		}
		pos = end + 2 // 2-space separator
	}
	return -1
}

// wireTabClicks sets mouse capture on tab bars to switch tabs on click.
func (v *View) wireTabClicks() {
	v.ReqTabBar.SetMouseCapture(func(action tview.MouseAction, ev *tcell.EventMouse) (tview.MouseAction, *tcell.EventMouse) {
		if action == tview.MouseLeftClick {
			screenX, _ := ev.Position()
			bx, _, _, _ := v.ReqTabBar.GetRect()
			idx := tabIndexAtX(reqTabNames, screenX-bx)
			if idx >= 0 {
				v.SetReqTab(idx)
			}
		}
		return action, ev
	})

	v.RespTabBar.SetMouseCapture(func(action tview.MouseAction, ev *tcell.EventMouse) (tview.MouseAction, *tcell.EventMouse) {
		if action == tview.MouseLeftClick {
			screenX, _ := ev.Position()
			bx, _, _, _ := v.RespTabBar.GetRect()
			idx := tabIndexAtX(respTabNames, screenX-bx)
			if idx >= 0 {
				v.SetRespTab(idx)
			}
		}
		return action, ev
	})
}

// SetReqTab switches the active request tab.
func (v *View) SetReqTab(index int) {
	if index < 0 || index >= len(reqTabNames) {
		return
	}
	v.ReqActiveTab = index
	v.renderReqTabBar()
	v.ReqPages.SwitchToPage(reqTabNames[index])
}

// SetRespTab switches the active response tab.
func (v *View) SetRespTab(index int) {
	if index < 0 || index >= len(respTabNames) {
		return
	}
	v.RespActiveTab = index
	v.renderRespTabBar()
	v.RespPages.SwitchToPage(respTabNames[index])
}

// UpdateRequestView populates the request panel from a parsed .http request.
func (v *View) UpdateRequestView(req *httpfile.Request) {
	// Headers tab
	if len(req.Headers) == 0 {
		v.ReqHeadersTv.SetText("\n\n\n[#4a4f72]No headers[-]")
		v.ReqHeadersTv.SetTextAlign(tview.AlignCenter)
	} else {
		var b strings.Builder
		for k, val := range req.Headers {
			fmt.Fprintf(&b, "[#a78bfa]%s[-]: [#d4d8e8]%s[-]\n", k, val)
		}
		v.ReqHeadersTv.SetText(b.String())
		v.ReqHeadersTv.SetTextAlign(tview.AlignLeft)
	}

	// Body tab
	if req.Body == "" {
		v.ReqBodyTv.SetText("\n\n\n[#4a4f72]No body[-]")
	} else {
		v.ReqBodyTv.SetText(req.Body)
	}

	// Params tab — parse URL query params into KVTable
	var pairs []widgets.KVPair
	if parsed, err := url.Parse(req.URL); err == nil {
		for key, vals := range parsed.Query() {
			for _, val := range vals {
				pairs = append(pairs, widgets.KVPair{Key: key, Value: val})
			}
		}
	}
	v.ReqParamsTable.SetPairs(pairs)

	// Update URL bar
	v.URLInput.SetText(req.URL)
	// Set method
	for i, opt := range []string{"GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"} {
		if opt == req.Method {
			v.Method.SetCurrentOption(i)
			break
		}
	}

	v.UpdateStatus(false)
}

// updateRespStatus updates the status code label and colored bar in the URL bar.
func (v *View) updateRespStatus(code int) {
	var color string
	switch {
	case code >= 200 && code < 300:
		color = "#22c55e"
	case code >= 300 && code < 400:
		color = "#f59e0b"
	case code >= 400 && code < 500:
		color = "#f87171"
	default:
		color = "#ef4444"
	}
	v.RespStatusCode.SetText(fmt.Sprintf("[%s] %d [-]", color, code))
	v.RespStatusBar.SetBackgroundColor(tcell.GetColor(color))
}

// UpdateResponseView populates the response panel.
// formattedBody is the pre-formatted response body (done off the main goroutine).
func (v *View) UpdateResponseView(resp *httpclient.Response, formattedBody string) {
	if resp.Err != nil {
		v.RespBodyTv.SetText(fmt.Sprintf("[red]Error: %v[-]", resp.Err))
		v.SetRespTab(0)
		v.UpdateStatus(false)
		return
	}
	v.updateRespStatus(resp.StatusCode)

	// Body tab
	uiDbg("SetText body start (%d bytes)", len(formattedBody))
	v.RespBodyTv.SetText(formattedBody)
	uiDbg("SetText body done")

	// Headers tab
	uiDbg("SetText headers start")
	var hb strings.Builder
	for k, vals := range resp.Headers {
		for _, val := range vals {
			fmt.Fprintf(&hb, "[#a78bfa]%s[-]: [#d4d8e8]%s[-]\n", k, val)
		}
	}
	v.RespHeadersTv.SetText(hb.String())
	uiDbg("SetText headers done")

	uiDbg("SetRespTab start")
	v.SetRespTab(0)
	uiDbg("SetRespTab done")
	v.UpdateStatus(false)
	uiDbg("UpdateStatus done — returning to tview Draw")
}

// SetCurrentFile updates the file shown in the status bar.
func (v *View) SetCurrentFile(path string) {
	v.CurrentFile = path
	v.UpdateStatus(false)
}

// UpdateStatus refreshes the status bar text.
func (v *View) UpdateStatus(sending bool) {
	v.StatusBar.SetText(statusBarText(v.CurrentFile, sending))
}

func statusBarText(file string, sending bool) string {
	key := func(k, label string) string {
		return fmt.Sprintf(" [#a78bfa]%s[-] [#8b90a8]%s[-]", k, label)
	}
	shortcuts := key("^c", "Quit") + key("^j", "Send") + key("^t", "Method") + key("^[", "Sidebar") + key("Tab", "Focus")
	state := ""
	if sending {
		state = "  [#a78bfa]Sending...[-]"
	}
	return shortcuts + state
}

// SetEnvOptions populates the env dropdown. Pass nil/empty to show "no env".
// Returns the selected index (always 0 when labels are present).
func (v *View) SetEnvOptions(labels []string) {
	noStyle := tcell.StyleDefault.Background(tcell.NewHexColor(0x1e293b)).Foreground(tcell.NewHexColor(0x64748b))
	activeStyle := tcell.StyleDefault.Background(tcell.NewHexColor(0x0d6b5e)).Foreground(tcell.NewHexColor(0x5eead4))

	if len(labels) == 0 {
		v.EnvDropDown.SetOptions([]string{"no env"}, func(_ string, _ int) {
			v.EnvDropDown.SetFieldStyle(noStyle)
			v.EnvDropDown.SetFocusedStyle(noStyle)
		})
		v.EnvDropDown.SetCurrentOption(0)
		return
	}

	v.EnvDropDown.SetOptions(labels, func(_ string, _ int) {
		v.EnvDropDown.SetFieldStyle(activeStyle)
		v.EnvDropDown.SetFocusedStyle(activeStyle)
	})
	v.EnvDropDown.SetCurrentOption(0)
}

// EnvSelectedIndex returns the currently selected env index from the dropdown.
func (v *View) EnvSelectedIndex() int {
	idx, _ := v.EnvDropDown.GetCurrentOption()
	return idx
}

// IsInReqPanel reports whether p is a focusable widget inside the request panel.
func (v *View) IsInReqPanel(p tview.Primitive) bool {
	return v.ReqParamsTable.ContainsFocus(p) || p == v.ReqHeadersTv || p == v.ReqBodyTv
}

// IsInRespPanel reports whether p is a focusable widget inside the response panel.
func (v *View) IsInRespPanel(p tview.Primitive) bool {
	return p == v.RespBodyTv || p == v.RespHeadersTv
}
