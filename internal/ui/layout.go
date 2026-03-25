package ui

import (
	"encoding/base64"
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
	ReqTabBar           *tview.TextView
	ReqTabUnderline     *tview.TextView
	ReqPages            *tview.Pages
	ReqHeadersTable     *widgets.KVTable
	ReqBodyTypeDropDown *tview.DropDown
	ReqBodyPages        *tview.Pages
	ReqBodyArea         *tview.TextArea
	ReqFormTable        *widgets.KVTable
	ReqBodyType         string
	ReqAuthTypeDropDown *tview.DropDown
	ReqAuthPages        *tview.Pages
	ReqAuthTable        *widgets.KVTable
	ReqAuthType         string
	ReqScriptsArea      *tview.TextArea
	ReqOptionsTable     *widgets.KVTable
	ReqParamsTable      *widgets.KVTable

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

	v.ReqHeadersTable = widgets.NewKVTable(app)

	v.ReqBodyArea = tview.NewTextArea()
	v.ReqBodyArea.SetBackgroundColor(ColorBg)
	v.ReqBodyArea.SetTextStyle(tcell.StyleDefault.Background(ColorBg).Foreground(ColorText))
	v.ReqBodyArea.SetBorder(false)

	v.ReqFormTable = widgets.NewKVTable(app)

	bodyNonePlaceholder := tview.NewTextView()
	bodyNonePlaceholder.SetDynamicColors(true)
	bodyNonePlaceholder.SetBackgroundColor(ColorBg)
	bodyNonePlaceholder.SetTextAlign(tview.AlignCenter)
	bodyNonePlaceholder.SetText("\n\n\n[#4a4f72]No body[-]")

	v.ReqBodyPages = tview.NewPages()
	v.ReqBodyPages.AddPage("none", bodyNonePlaceholder, true, true)
	v.ReqBodyPages.AddPage("raw", v.ReqBodyArea, true, false)
	v.ReqBodyPages.AddPage("form", v.ReqFormTable.Widget(), true, false)

	v.ReqBodyType = "No Body"
	v.ReqBodyTypeDropDown = tview.NewDropDown()
	v.ReqBodyTypeDropDown.SetBackgroundColor(ColorBg)
	v.ReqBodyTypeDropDown.SetTextOptions(" ", " ", " ", "", "")
	v.ReqBodyTypeDropDown.SetListStyles(
		tcell.StyleDefault.Background(ColorBgPanel).Foreground(ColorText),
		tcell.StyleDefault.Background(tcell.NewHexColor(0x5b21b6)).Foreground(tcell.ColorWhite),
	)
	v.ReqBodyTypeDropDown.SetOptions(bodyTypeOptions, func(text string, _ int) {
		v.ReqBodyTypeDropDown.SetFieldStyle(tcell.StyleDefault.Background(ColorBg).Foreground(ColorTextDim))
		v.switchBodyType(text)
	})
	v.ReqBodyTypeDropDown.SetFieldStyle(tcell.StyleDefault.Background(ColorBg).Foreground(ColorTextDim))
	v.ReqBodyTypeDropDown.SetCurrentOption(0) // "No Body"

	bodyTypeBar := tview.NewFlex().SetDirection(tview.FlexColumn)
	bodyTypeBar.SetBackgroundColor(ColorBg)
	bodyTypeBar.AddItem(tview.NewBox().SetBackgroundColor(ColorBg), 0, 1, false)
	bodyTypeBar.AddItem(v.ReqBodyTypeDropDown, 22, 0, false)

	bodyTab := tview.NewFlex().SetDirection(tview.FlexRow)
	bodyTab.SetBackgroundColor(ColorBg)
	bodyTab.AddItem(bodyTypeBar, 1, 0, false)
	bodyTab.AddItem(v.ReqBodyPages, 0, 1, true)

	v.ReqAuthTable = widgets.NewKVTable(app)
	v.ReqAuthType = "Inherit"

	authNonePlaceholder := tview.NewTextView()
	authNonePlaceholder.SetDynamicColors(true)
	authNonePlaceholder.SetBackgroundColor(ColorBg)
	authNonePlaceholder.SetTextAlign(tview.AlignCenter)
	authNonePlaceholder.SetText("\n\n\n[#4a4f72]No auth[-]")

	v.ReqAuthPages = tview.NewPages()
	v.ReqAuthPages.AddPage("none", authNonePlaceholder, true, false)
	v.ReqAuthPages.AddPage("fields", v.ReqAuthTable.Widget(), true, false)
	v.ReqAuthPages.SwitchToPage("none")

	v.ReqAuthTypeDropDown = tview.NewDropDown()
	v.ReqAuthTypeDropDown.SetBackgroundColor(ColorBg)
	v.ReqAuthTypeDropDown.SetTextOptions(" ", " ", " ", "", "")
	v.ReqAuthTypeDropDown.SetListStyles(
		tcell.StyleDefault.Background(ColorBgPanel).Foreground(ColorText),
		tcell.StyleDefault.Background(tcell.NewHexColor(0x5b21b6)).Foreground(tcell.ColorWhite),
	)
	v.ReqAuthTypeDropDown.SetOptions(authTypeOptions, func(text string, _ int) {
		v.ReqAuthTypeDropDown.SetFieldStyle(tcell.StyleDefault.Background(ColorBg).Foreground(ColorTextDim))
		v.switchAuthType(text)
	})
	v.ReqAuthTypeDropDown.SetFieldStyle(tcell.StyleDefault.Background(ColorBg).Foreground(ColorTextDim))
	// Default to "Inherit"
	for i, opt := range authTypeOptions {
		if opt == "Inherit" {
			v.ReqAuthTypeDropDown.SetCurrentOption(i)
			break
		}
	}

	authTypeBar := tview.NewFlex().SetDirection(tview.FlexColumn)
	authTypeBar.SetBackgroundColor(ColorBg)
	authTypeBar.AddItem(tview.NewBox().SetBackgroundColor(ColorBg), 0, 1, false)
	authTypeBar.AddItem(v.ReqAuthTypeDropDown, 22, 0, false)

	authTab := tview.NewFlex().SetDirection(tview.FlexRow)
	authTab.SetBackgroundColor(ColorBg)
	authTab.AddItem(authTypeBar, 1, 0, false)
	authTab.AddItem(v.ReqAuthPages, 0, 1, true)

	v.ReqScriptsArea = tview.NewTextArea()
	v.ReqScriptsArea.SetBackgroundColor(ColorBg)
	v.ReqScriptsArea.SetTextStyle(tcell.StyleDefault.Background(ColorBg).Foreground(ColorText))
	v.ReqScriptsArea.SetBorder(false)

	v.ReqOptionsTable = widgets.NewKVTable(app)

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

	infoStub := tview.NewTextView()
	infoStub.SetDynamicColors(true)
	infoStub.SetBackgroundColor(ColorBg)
	infoStub.SetTextAlign(tview.AlignCenter)
	infoStub.SetText("\n\n\n[#4a4f72]Info[-]")

	v.ReqPages = tview.NewPages()
	v.ReqPages.AddPage("Params", v.ReqParamsTable.Widget(), true, true)
	v.ReqPages.AddPage("Headers", v.ReqHeadersTable.Widget(), true, false)
	v.ReqPages.AddPage("Body", bodyTab, true, false)
	v.ReqPages.AddPage("Auth", authTab, true, false)
	v.ReqPages.AddPage("Info", infoStub, true, false)
	v.ReqPages.AddPage("Scripts", v.ReqScriptsArea, true, false)
	v.ReqPages.AddPage("Options", v.ReqOptionsTable.Widget(), true, false)

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
	mainRow.AddItem(v.Sidebar, 48, 0, true)
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
	// Keep ReqFocusWidget in sync so Tab cycling lands on the right widget.
	switch reqTabNames[index] {
	case "Headers":
		v.ReqFocusWidget = v.ReqHeadersTable.Widget()
	case "Body":
		v.ReqFocusWidget = v.bodyFocusWidget()
	case "Auth":
		v.ReqFocusWidget = v.authFocusWidget()
	case "Scripts":
		v.ReqFocusWidget = v.ReqScriptsArea
	case "Options":
		v.ReqFocusWidget = v.ReqOptionsTable.Widget()
	default:
		v.ReqFocusWidget = v.ReqParamsTable.Widget()
	}
}

// SetRespTab switches the active response tab.
func (v *View) SetRespTab(index int) {
	if index < 0 || index >= len(respTabNames) {
		return
	}
	v.RespActiveTab = index
	v.renderRespTabBar()
	v.RespPages.SwitchToPage(respTabNames[index])
	// Keep RespFocusWidget in sync so Tab cycling lands on the right widget.
	if respTabNames[index] == "Headers" {
		v.RespFocusWidget = v.RespHeadersTv
	} else {
		v.RespFocusWidget = v.RespBodyTv
	}
}

// UpdateRequestView populates the request panel from a parsed .http request.
func (v *View) UpdateRequestView(req *httpfile.Request) {
	// Headers tab
	var headerPairs []widgets.KVPair
	for k, val := range req.Headers {
		headerPairs = append(headerPairs, widgets.KVPair{Key: k, Value: val})
	}
	v.ReqHeadersTable.SetPairs(headerPairs)

	// Body tab — detect type from Content-Type header, then populate content.
	bodyType := detectBodyType(req.Headers["Content-Type"], req.Body)
	v.ReqBodyArea.SetText("", false)
	v.ReqFormTable.SetPairs(nil)
	switch bodyType {
	case "Form URL Encoded", "Multipart Form":
		// Parse form-encoded body back into pairs.
		if vals, err := url.ParseQuery(req.Body); err == nil {
			var pairs []widgets.KVPair
			for k, vs := range vals {
				for _, val := range vs {
					pairs = append(pairs, widgets.KVPair{Key: k, Value: val})
				}
			}
			v.ReqFormTable.SetPairs(pairs)
		}
	default:
		v.ReqBodyArea.SetText(req.Body, false)
	}
	// Update dropdown without re-triggering onChange (set index directly).
	for i, opt := range bodyTypeOptions {
		if opt == bodyType {
			v.ReqBodyTypeDropDown.SetCurrentOption(i)
			break
		}
	}
	v.ReqBodyType = bodyType
	switch bodyType {
	case "No Body":
		v.ReqBodyPages.SwitchToPage("none")
	case "Form URL Encoded", "Multipart Form":
		v.ReqBodyPages.SwitchToPage("form")
	default:
		v.ReqBodyPages.SwitchToPage("raw")
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
	shortcuts := key("^c", "Quit") + key("^j", "Send") + key("^t", "Method") + key("^[", "Sidebar") + key("Tab", "Focus") + key("[/]", "Tabs")
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

var bodyTypeOptions = []string{
	"No Body",
	"JSON", "Form URL Encoded", "Multipart Form",
	"XML", "TEXT", "SPARQL",
}

// switchBodyType updates the active body sub-page and focus widget.
func (v *View) switchBodyType(bodyType string) {
	v.ReqBodyType = bodyType
	switch bodyType {
	case "No Body":
		v.ReqBodyPages.SwitchToPage("none")
	case "Form URL Encoded", "Multipart Form":
		v.ReqBodyPages.SwitchToPage("form")
	default:
		v.ReqBodyPages.SwitchToPage("raw")
	}
	v.ReqFocusWidget = v.bodyFocusWidget()
}

// bodyFocusWidget returns the focusable widget for the currently selected body type.
func (v *View) bodyFocusWidget() tview.Primitive {
	switch v.ReqBodyType {
	case "No Body":
		return v.ReqBodyTypeDropDown
	case "Form URL Encoded", "Multipart Form":
		return v.ReqFormTable.Widget()
	default:
		return v.ReqBodyArea
	}
}

// GetBody returns the serialised request body based on the selected body type.
func (v *View) GetBody() string {
	switch v.ReqBodyType {
	case "No Body":
		return ""
	case "Form URL Encoded", "Multipart Form":
		vals := url.Values{}
		for _, p := range v.ReqFormTable.GetPairs() {
			if p.Key != "" {
				vals.Set(p.Key, p.Value)
			}
		}
		return vals.Encode()
	default:
		return v.ReqBodyArea.GetText()
	}
}

// GetBodyContentType returns the Content-Type implied by the selected body type,
// or "" for No Body (caller should not set the header in that case).
func (v *View) GetBodyContentType() string {
	switch v.ReqBodyType {
	case "JSON":
		return "application/json"
	case "XML":
		return "application/xml"
	case "TEXT":
		return "text/plain"
	case "SPARQL":
		return "application/sparql-query"
	case "Form URL Encoded":
		return "application/x-www-form-urlencoded"
	case "Multipart Form":
		return "multipart/form-data"
	default:
		return ""
	}
}

// GetAuth converts the current auth fields into HTTP headers ready to send.
func (v *View) GetAuth() map[string]string {
	pairMap := make(map[string]string)
	for _, p := range v.ReqAuthTable.GetPairs() {
		if p.Key != "" {
			pairMap[p.Key] = p.Value
		}
	}
	switch v.ReqAuthType {
	case "Bearer Token":
		if token := pairMap["Token"]; token != "" {
			return map[string]string{"Authorization": "Bearer " + token}
		}
	case "Basic Auth":
		user, pass := pairMap["Username"], pairMap["Password"]
		encoded := base64.StdEncoding.EncodeToString([]byte(user + ":" + pass))
		return map[string]string{"Authorization": "Basic " + encoded}
	case "API Key":
		if k, v := pairMap["Key"], pairMap["Value"]; k != "" {
			return map[string]string{k: v}
		}
	case "AWS Sig v4", "Digest Auth", "NTLM Auth", "WSSE Auth", "OAuth 2.0":
		// Pass raw pairs through as headers (full signing not implemented).
		if len(pairMap) > 0 {
			return pairMap
		}
	}
	return nil
}

// GetHeaders returns the current request headers from the interactive headers table.
func (v *View) GetHeaders() map[string]string {
	pairs := v.ReqHeadersTable.GetPairs()
	if len(pairs) == 0 {
		return nil
	}
	m := make(map[string]string, len(pairs))
	for _, p := range pairs {
		if p.Key != "" {
			m[p.Key] = p.Value
		}
	}
	return m
}

var authTypeOptions = []string{
	"Inherit", "No Auth",
	"Bearer Token", "Basic Auth", "API Key",
	"AWS Sig v4", "Digest Auth", "NTLM Auth", "WSSE Auth", "OAuth 2.0",
}

// authTypeFields returns the predefined KVPair keys for each auth type.
func authTypeFields(authType string) []widgets.KVPair {
	switch authType {
	case "Bearer Token":
		return []widgets.KVPair{{Key: "Token", Value: ""}}
	case "Basic Auth", "Digest Auth", "NTLM Auth", "WSSE Auth":
		return []widgets.KVPair{{Key: "Username", Value: ""}, {Key: "Password", Value: ""}}
	case "API Key":
		return []widgets.KVPair{{Key: "Key", Value: ""}, {Key: "Value", Value: ""}}
	case "AWS Sig v4":
		return []widgets.KVPair{
			{Key: "Access Key", Value: ""},
			{Key: "Secret Key", Value: ""},
			{Key: "AWS Region", Value: ""},
			{Key: "AWS Service", Value: ""},
		}
	case "OAuth 2.0":
		return []widgets.KVPair{{Key: "Access Token", Value: ""}}
	}
	return nil
}

// switchAuthType updates the active auth sub-page and populates the field table.
func (v *View) switchAuthType(authType string) {
	v.ReqAuthType = authType
	fields := authTypeFields(authType)
	if fields == nil {
		v.ReqAuthPages.SwitchToPage("none")
		v.ReqFocusWidget = v.ReqAuthTypeDropDown
	} else {
		v.ReqAuthTable.SetPairs(fields)
		v.ReqAuthPages.SwitchToPage("fields")
		v.ReqFocusWidget = v.ReqAuthTable.Widget()
	}
}

// authFocusWidget returns the right focus target for the current auth type.
func (v *View) authFocusWidget() tview.Primitive {
	if authTypeFields(v.ReqAuthType) == nil {
		return v.ReqAuthTypeDropDown
	}
	return v.ReqAuthTable.Widget()
}

// detectBodyType returns the body type string for the given Content-Type and body.
func detectBodyType(contentType, body string) string {
	ct := strings.ToLower(strings.SplitN(contentType, ";", 2)[0])
	ct = strings.TrimSpace(ct)
	switch ct {
	case "application/json":
		return "JSON"
	case "application/xml", "text/xml":
		return "XML"
	case "text/plain":
		return "TEXT"
	case "application/sparql-query":
		return "SPARQL"
	case "application/x-www-form-urlencoded":
		return "Form URL Encoded"
	case "multipart/form-data":
		return "Multipart Form"
	}
	if body != "" {
		return "JSON" // sensible default for non-empty bodies without a Content-Type
	}
	return "No Body"
}

// IsInReqPanel reports whether p is a focusable widget inside the request panel.
func (v *View) IsInReqPanel(p tview.Primitive) bool {
	return v.ReqParamsTable.ContainsFocus(p) ||
		v.ReqHeadersTable.ContainsFocus(p) ||
		v.ReqAuthTable.ContainsFocus(p) ||
		v.ReqOptionsTable.ContainsFocus(p) ||
		v.ReqFormTable.ContainsFocus(p) ||
		p == v.ReqBodyTypeDropDown ||
		p == v.ReqAuthTypeDropDown ||
		p == v.ReqBodyArea ||
		p == v.ReqScriptsArea
}

// IsInReqPanelNav reports whether p is in the request panel on a widget where
// [ and ] can safely be used for tab navigation (i.e. not a text input area).
func (v *View) IsInReqPanelNav(p tview.Primitive) bool {
	return v.ReqParamsTable.ContainsFocus(p) ||
		v.ReqHeadersTable.ContainsFocus(p) ||
		v.ReqAuthTable.ContainsFocus(p) ||
		v.ReqOptionsTable.ContainsFocus(p) ||
		v.ReqFormTable.ContainsFocus(p) ||
		p == v.ReqBodyTypeDropDown ||
		p == v.ReqAuthTypeDropDown
}

// IsInRespPanel reports whether p is a focusable widget inside the response panel.
func (v *View) IsInRespPanel(p tview.Primitive) bool {
	return p == v.RespBodyTv || p == v.RespHeadersTv
}
