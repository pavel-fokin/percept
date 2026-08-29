package app

import (
	"context"

	"github.com/pavel-fokin/percept/go/internal/percept"
)

type Conversation struct {
	events []percept.Event
	chat   percept.Model
}

func NewConversation(chat percept.Model) *Conversation {
	return &Conversation{chat: chat}
}

// Submit records the user's message and returns a thunk that computes
// the assistant's reply. The history it needs is captured now, before
// returning - the thunk touches no shared state, so it's safe to run on
// any goroutine.
func (c *Conversation) Submit(ctx context.Context, text string) (func() string, error) {
	userEvent, err := percept.NewEvent(percept.SenderUser, text)
	if err != nil {
		return nil, err
	}
	c.events = append(c.events, userEvent)

	history := percept.ToMessages(c.events)
	return func() string {
		reply, err := c.chat.Reply(ctx, history)
		if err != nil {
			return "Sorry, something went wrong."
		}
		return reply
	}, nil
}

// AppendReply records an assistant reply. Must only be called from the
// goroutine that owns the Conversation (tui's event loop) - never from
// inside the thunk Submit returns.
func (c *Conversation) AppendReply(content string) error {
	event, err := percept.NewEvent(percept.SenderAssistant, content)
	if err != nil {
		return err
	}
	c.events = append(c.events, event)
	return nil
}

func (c *Conversation) Events() []percept.Event { return c.events }
