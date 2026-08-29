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

// Model turns a conversation into a streamed reply - the domain's core
// capability, mechanism-agnostic. Reply returns as soon as streaming has
// started (not once it's finished); chunks arrive on the returned
// channel as they're produced, and the channel is closed when the reply
// is complete. Mid-stream errors are out of scope - only a failure
// before streaming starts is reported via the returned error.
type Model interface {
	Reply(ctx context.Context, messages []Message) (<-chan string, error)
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
