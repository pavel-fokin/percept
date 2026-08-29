package main

import (
	"time"

	"github.com/pavel-fokin/percept/go/llm"
)

type sender int

const (
	senderUser sender = iota
	senderAssistant
)

// EventID identifies an Event.
type EventID = ID[Event]

// Event records one chat message: who sent it, what it said, and when.
// Events are the app's only record of the transcript - they're kept
// in memory, in the model's events slice, for the life of the process.
type Event struct {
	id        EventID
	createdAt time.Time
	sender    sender
	content   string
}

func newEvent(from sender, content string) (Event, error) {
	id, err := NewID[Event]()
	if err != nil {
		return Event{}, err
	}
	return Event{
		id:        id,
		createdAt: time.Now(),
		sender:    from,
		content:   content,
	}, nil
}

// toMessages converts the transcript into the provider-agnostic form the
// llm package expects.
func toMessages(events []Event) []llm.Message {
	messages := make([]llm.Message, len(events))
	for i, e := range events {
		role := llm.RoleUser
		if e.sender == senderAssistant {
			role = llm.RoleAssistant
		}
		messages[i] = llm.Message{Role: role, Content: e.content}
	}
	return messages
}
