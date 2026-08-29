package app

import (
	"context"

	"github.com/pavel-fokin/percept/go/internal/percept"
)

type Conversation struct {
	events []percept.Event
	chat   percept.Model
	// streaming is the index into events of the assistant event
	// currently receiving chunks, or -1 if none is in progress.
	streaming int
}

func NewConversation(chat percept.Model) *Conversation {
	return &Conversation{chat: chat, streaming: -1}
}

// Submit records the user's message and starts the reply stream,
// returning the channel chunks arrive on.
func (c *Conversation) Submit(ctx context.Context, text string) (<-chan string, error) {
	userEvent, err := percept.NewEvent(percept.SenderUser, text)
	if err != nil {
		return nil, err
	}
	c.events = append(c.events, userEvent)

	history := percept.ToMessages(c.events)
	chunks, err := c.chat.Reply(ctx, history)
	if err != nil {
		fallback := make(chan string, 1)
		fallback <- "Sorry, something went wrong."
		close(fallback)
		return fallback, nil
	}
	return chunks, nil
}

// AppendChunk appends a chunk to the in-progress assistant reply,
// starting a new assistant event on the first chunk of a stream. Must
// only be called from the goroutine that owns the Conversation - never
// from inside the stream's producer.
func (c *Conversation) AppendChunk(chunk string) error {
	if c.streaming < 0 {
		event, err := percept.NewEvent(percept.SenderAssistant, chunk)
		if err != nil {
			return err
		}
		c.events = append(c.events, event)
		c.streaming = len(c.events) - 1
		return nil
	}
	c.events[c.streaming].Content += chunk
	return nil
}

// EndStream marks the in-progress reply complete, so the next chunk
// received starts a new assistant event instead of extending this one.
func (c *Conversation) EndStream() {
	c.streaming = -1
}

func (c *Conversation) Events() []percept.Event { return c.events }
