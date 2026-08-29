package main

import "time"

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

func stubAssistantReply(userText string) string {
	return "You said: " + userText
}
