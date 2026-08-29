package tui

import (
	"context"
	"strings"

	"charm.land/bubbles/v2/cursor"
	tea "charm.land/bubbletea/v2"
)

type chunkMsg struct {
	content string
	chunks  <-chan string
}

type streamDoneMsg struct{}

// waitForChunk reads one chunk off chunks. Handed back as the next Cmd
// from handleChunk, it keeps re-arming itself until the channel closes.
func waitForChunk(chunks <-chan string) tea.Cmd {
	return func() tea.Msg {
		chunk, ok := <-chunks
		if !ok {
			return streamDoneMsg{}
		}
		return chunkMsg{content: chunk, chunks: chunks}
	}
}

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		return m.handleResize(msg), nil
	case tea.KeyPressMsg:
		return m.handleKeyPress(msg)
	case cursor.BlinkMsg:
		return m.handleCursorBlink(msg)
	case chunkMsg:
		return m.handleChunk(msg)
	case streamDoneMsg:
		m.app.EndStream()
		return m, nil
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
// starts pulling chunks off the reply stream.
func (m model) submit() (tea.Model, tea.Cmd) {
	text := strings.TrimSpace(m.textarea.Value())
	if text == "" {
		return m, nil
	}
	chunks, err := m.app.Submit(context.Background(), text)
	if err != nil {
		return m, nil
	}
	m.viewport.SetContent(m.renderTranscript())
	m.textarea.Reset()
	m.viewport.GotoBottom()

	return m, waitForChunk(chunks)
}

// handleChunk appends the chunk, re-renders, and re-arms the read for
// the next one.
func (m model) handleChunk(msg chunkMsg) (tea.Model, tea.Cmd) {
	if err := m.app.AppendChunk(msg.content); err != nil {
		return m, nil
	}
	m.viewport.SetContent(m.renderTranscript())
	m.viewport.GotoBottom()
	return m, waitForChunk(msg.chunks)
}
