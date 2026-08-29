// Package llm defines a provider-agnostic interface for chatting with an
// LLM. Provider-specific implementations live in the providers package.
package llm

import "context"

// Role identifies who authored a Message, independent of any app-specific
// entity - a Model shouldn't need to know about this app's domain types.
type Role int

const (
	RoleUser Role = iota
	RoleAssistant
)

// Message is one turn in a conversation.
type Message struct {
	Role    Role
	Content string
}

// Model turns a conversation into a reply. Kept synchronous for now - a
// provider backed by a real network call will need to move this behind a
// tea.Cmd so it doesn't block the UI; that's a deliberate follow-up, not
// an oversight.
type Model interface {
	Reply(ctx context.Context, messages []Message) (string, error)
}
