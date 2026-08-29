package main

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
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
	lines := make([]string, 0, len(m.messages))
	for _, msg := range m.messages {
		lines = append(lines, m.renderLine(msg))
	}
	return lipgloss.NewStyle().Width(m.viewport.Width()).Render(strings.Join(lines, "\n"))
}

func (m model) renderLine(msg chatMessage) string {
	style, prefix := m.userStyle, "You: "
	if msg.from == senderAssistant {
		style, prefix = m.assistantStyle, "Assistant: "
	}
	return style.Render(prefix) + msg.text
}
