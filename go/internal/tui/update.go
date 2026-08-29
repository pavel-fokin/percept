package tui

import (
	"context"
	"strings"

	"charm.land/bubbles/v2/cursor"
	tea "charm.land/bubbletea/v2"
)

type replyMsg string

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		return m.handleResize(msg), nil
	case tea.KeyPressMsg:
		return m.handleKeyPress(msg)
	case cursor.BlinkMsg:
		return m.handleCursorBlink(msg)
	case replyMsg:
		return m.handleReply(msg), nil
	}
	return m, nil
}

func (m model) handleResize(msg tea.WindowSizeMsg) model {
	m.textarea.SetWidth(msg.Width)
	m.viewport.SetWidth(msg.Width)
	m.viewport.SetHeight(msg.Height - m.textarea.Height() - 1)
	m.viewport.SetContent(m.renderTranscript())
	m.viewport.GotoBottom()
	m.ready = true
	return m
}

func (m model) handleKeyPress(msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "ctrl+c", "esc":
		return m, tea.Quit
	case "enter":
		return m.submit()
	default:
		var cmd tea.Cmd
		m.textarea, cmd = m.textarea.Update(msg)
		return m, cmd
	}
}

func (m model) handleCursorBlink(msg cursor.BlinkMsg) (tea.Model, tea.Cmd) {
	var cmd tea.Cmd
	m.textarea, cmd = m.textarea.Update(msg)
	return m, cmd
}

// submit sends the user's message immediately (visible right away), then
// returns a Cmd that fetches the reply on its own goroutine. Bails
// without resetting if the app layer errors, same as before.
func (m model) submit() (tea.Model, tea.Cmd) {
	text := strings.TrimSpace(m.textarea.Value())
	if text == "" {
		return m, nil
	}
	fetch, err := m.app.Submit(context.Background(), text)
	if err != nil {
		return m, nil
	}
	m.viewport.SetContent(m.renderTranscript())
	m.textarea.Reset()
	m.viewport.GotoBottom()

	return m, func() tea.Msg { return replyMsg(fetch()) }
}

// handleReply appends the fetched reply and re-renders. The only place
// AppendReply is called - always on Bubble Tea's event-loop goroutine.
func (m model) handleReply(msg replyMsg) model {
	if err := m.app.AppendReply(string(msg)); err != nil {
		return m
	}
	m.viewport.SetContent(m.renderTranscript())
	m.viewport.GotoBottom()
	return m
}
