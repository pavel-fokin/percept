package providers

import (
	"context"

	"github.com/pavel-fokin/percept/go/internal/percept"
)

// Stub echoes the last user message back, prefixed.
type Stub struct{}

func (Stub) Reply(_ context.Context, messages []percept.Message) (string, error) {
	for i := len(messages) - 1; i >= 0; i-- {
		if messages[i].Role == percept.RoleUser {
			return "You said: " + messages[i].Content, nil
		}
	}
	return "", nil
}
