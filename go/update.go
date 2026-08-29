package main

import (
	"strings"

	"charm.land/bubbles/v2/cursor"
	tea "charm.land/bubbletea/v2"
)

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		return m.handleResize(msg), nil
	case tea.KeyPressMsg:
		return m.handleKeyPress(msg)
	case cursor.BlinkMsg:
		return m.handleCursorBlink(msg)
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
		return m.submit(), nil
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

// submit appends the pending input as a user event plus an immediate
// stub assistant reply, then resets the input and scrolls to the bottom.
func (m model) submit() model {
	text := strings.TrimSpace(m.textarea.Value())
	if text == "" {
		return m
	}

	userEvent, err := newEvent(senderUser, text)
	if err != nil {
		return m
	}
	assistantEvent, err := newEvent(senderAssistant, stubAssistantReply(text))
	if err != nil {
		return m
	}

	m.events = append(m.events, userEvent, assistantEvent)
	m.viewport.SetContent(m.renderTranscript())
	m.textarea.Reset()
	m.viewport.GotoBottom()
	return m
}
