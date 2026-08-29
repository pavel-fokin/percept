package app

import (
	"context"

	"github.com/pavel-fokin/percept/go/internal/percept"
)

// Conversation orchestrates a chat: turns input into domain events, asks
// the configured Model for a reply, keeps the transcript. Pure
// orchestration - no vocabulary beyond percept's.
type Conversation struct {
	events []percept.Event
	chat   percept.Model
}

func NewConversation(chat percept.Model) *Conversation {
	return &Conversation{chat: chat}
}

func (c *Conversation) Submit(ctx context.Context, text string) error {
	userEvent, err := percept.NewEvent(percept.SenderUser, text)
	if err != nil {
		return err
	}
	c.events = append(c.events, userEvent)

	reply, err := c.chat.Reply(ctx, percept.ToMessages(c.events))
	if err != nil {
		reply = "Sorry, something went wrong."
	}

	assistantEvent, err := percept.NewEvent(percept.SenderAssistant, reply)
	if err != nil {
		return err
	}
	c.events = append(c.events, assistantEvent)
	return nil
}

func (c *Conversation) Events() []percept.Event { return c.events }
