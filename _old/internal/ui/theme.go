package ui

import (
	"github.com/gdamore/tcell/v2"
	"github.com/rivo/tview"
)

// Color palette — dark purple/navy theme matching Posting.
var (
	ColorBg          = tcell.NewHexColor(0x0d0f1e) // slightly warmer dark navy
	ColorBgPanel     = tcell.NewHexColor(0x0f1122)
	ColorBorder      = tcell.NewHexColor(0x2a2f4a) // brighter borders
	ColorText        = tcell.NewHexColor(0xd4d8e8)
	ColorTextDim     = tcell.NewHexColor(0x6b7090) // brighter labels/dim text
	ColorTextMuted   = tcell.NewHexColor(0x343858)
	ColorAccentFg    = tcell.NewHexColor(0xa78bfa)
	ColorMethodBg    = tcell.NewHexColor(0x14532d)
	ColorMethodFg    = tcell.NewHexColor(0x86efac)
	ColorSendBg      = tcell.NewHexColor(0x7c3aed)
	ColorSendFg      = tcell.NewHexColor(0xede9fe)
	ColorStatusBg    = ColorBg
	ColorStatusFg    = tcell.NewHexColor(0x525870)
	ColorTabActive   = tcell.NewHexColor(0xa78bfa)
	ColorTabInactive = tcell.NewHexColor(0x4a4f72) // slightly more visible inactive tabs
)

// ApplyTheme sets global tview defaults to the dark purple theme.
func ApplyTheme() {
	tview.Styles = tview.Theme{
		PrimitiveBackgroundColor:    ColorBg,
		ContrastBackgroundColor:     ColorBgPanel,
		MoreContrastBackgroundColor: ColorBorder,
		BorderColor:                 ColorBorder,
		TitleColor:                  ColorTextDim,
		GraphicsColor:               ColorBorder,
		PrimaryTextColor:            ColorText,
		SecondaryTextColor:          ColorTextDim,
		TertiaryTextColor:           ColorTextMuted,
		InverseTextColor:            ColorBg,
		ContrastSecondaryTextColor:  ColorAccentFg,
	}
}
