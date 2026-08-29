package main

import (
	"charm.land/bubbles/v2/textarea"
	"charm.land/bubbles/v2/viewport"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/pavel-fokin/percept/go/llm"
	"github.com/pavel-fokin/percept/go/providers"
)

type model struct {
	viewport       viewport.Model
	textarea       textarea.Model
	events         []Event
	chat           llm.Model
	userStyle      lipgloss.Style
	assistantStyle lipgloss.Style
	ready          bool
}

func initialModel() model {
	return model{
		textarea:       newTextarea(),
		viewport:       newViewport(),
		chat:           providers.Stub{},
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
