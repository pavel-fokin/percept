package main

import (
	"fmt"
	"os"

	tea "charm.land/bubbletea/v2"

	"github.com/pavel-fokin/percept/go/internal/app"
	"github.com/pavel-fokin/percept/go/internal/providers"
	"github.com/pavel-fokin/percept/go/internal/tui"
)

func main() {
	conversation := app.NewConversation(providers.Stub{})
	p := tea.NewProgram(tui.New(conversation))
	if _, err := p.Run(); err != nil {
		fmt.Fprintln(os.Stderr, "error running program:", err)
		os.Exit(1)
	}
}
