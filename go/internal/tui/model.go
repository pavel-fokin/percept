package tui

import (
	"context"

	"charm.land/bubbles/v2/textarea"
	"charm.land/bubbles/v2/viewport"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/pavel-fokin/percept/go/internal/percept"
)

// App is what tui needs from the application layer - defined here so tui
// depends on a narrow contract, not app's concrete Conversation type.
type App interface {
	Submit(ctx context.Context, text string) error
	Events() []percept.Event
}

type model struct {
	viewport       viewport.Model
	textarea       textarea.Model
	app            App
	userStyle      lipgloss.Style
	assistantStyle lipgloss.Style
	ready          bool
}

// New builds the chat TUI, backed by the given application layer.
func New(app App) tea.Model {
	return model{
		textarea:       newTextarea(),
		viewport:       newViewport(),
		app:            app,
		userStyle:      lipgloss.NewStyle().Foreground(lipgloss.Color("5")).Bold(true),
		assistantStyle: lipgloss.NewStyle().Foreground(lipgloss.Color("2")).Bold(true),
	}
}

func newTextarea() textarea.Model {
	ta := textarea.New()
	ta.Placeholder = "Type a message and press Enter..."
	ta.Focus()
	ta.Prompt = "┃ "
	ta.CharLimit = 500
	ta.SetHeight(1)
	ta.ShowLineNumbers = false
	ta.KeyMap.InsertNewline.SetEnabled(false)
	return ta
}

func newViewport() viewport.Model {
	vp := viewport.New()
	vp.KeyMap.Left.SetEnabled(false)
	vp.KeyMap.Right.SetEnabled(false)
	return vp
}

func (m model) Init() tea.Cmd { return textarea.Blink }
