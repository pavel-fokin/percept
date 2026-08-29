package tui

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/pavel-fokin/percept/go/internal/percept"
)

func (m model) View() tea.View {
	if !m.ready {
		return tea.NewView("Initializing...")
	}
	vpView := m.viewport.View()
	v := tea.NewView(vpView + "\n" + m.textarea.View())
	v.AltScreen = true
	if c := m.textarea.Cursor(); c != nil {
		c.Y += lipgloss.Height(vpView)
		v.Cursor = c
	}
	return v
}

func (m model) renderTranscript() string {
	events := m.app.Events()
	lines := make([]string, 0, len(events))
	for _, e := range events {
		lines = append(lines, m.renderLine(e))
	}
	return lipgloss.NewStyle().Width(m.viewport.Width()).Render(strings.Join(lines, "\n"))
}

func (m model) renderLine(e percept.Event) string {
	style, prefix := m.userStyle, "You: "
	if e.Sender == percept.SenderAssistant {
		style, prefix = m.assistantStyle, "Assistant: "
	}
	return style.Render(prefix) + e.Content
}
