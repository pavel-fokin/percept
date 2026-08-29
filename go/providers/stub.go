// Package providers holds concrete llm.Model implementations.
package providers

import (
	"context"

	"github.com/pavel-fokin/percept/go/llm"
)

// Stub echoes the last user message back, prefixed. Useful for exercising
// the chat UI without a real API key or network access.
type Stub struct{}

func (Stub) Reply(_ context.Context, messages []llm.Message) (string, error) {
	for i := len(messages) - 1; i >= 0; i-- {
		if messages[i].Role == llm.RoleUser {
			return "You said: " + messages[i].Content, nil
		}
	}
	return "", nil
}
