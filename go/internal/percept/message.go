package percept

import "context"

// Role identifies who authored a Message.
type Role int

const (
	RoleUser Role = iota
	RoleAssistant
)

// Message is one turn in a conversation - the value-object shape Model
// needs, independent of Event's identity/audit concerns.
type Message struct {
	Role    Role
	Content string
}

// Model turns a conversation into a reply - the domain's core capability,
// mechanism-agnostic. Kept synchronous for now; a network-backed provider
// will need to move this behind a tea.Cmd so it doesn't block the UI -
// a deliberate follow-up, not an oversight.
type Model interface {
	Reply(ctx context.Context, messages []Message) (string, error)
}

// ToMessages converts the transcript into the form Model expects.
func ToMessages(events []Event) []Message {
	messages := make([]Message, len(events))
	for i, e := range events {
		role := RoleUser
		if e.Sender == SenderAssistant {
			role = RoleAssistant
		}
		messages[i] = Message{Role: role, Content: e.Content}
	}
	return messages
}
