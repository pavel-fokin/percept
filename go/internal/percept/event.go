package percept

import "time"

// Sender identifies who sent an Event.
type Sender int

const (
	SenderUser Sender = iota
	SenderAssistant
)

// EventID identifies an Event.
type EventID = ID[Event]

// Event records one chat message: who sent it, what it said, and when.
type Event struct {
	ID        EventID
	CreatedAt time.Time
	Sender    Sender
	Content   string
}

func NewEvent(from Sender, content string) (Event, error) {
	id, err := NewID[Event]()
	if err != nil {
		return Event{}, err
	}
	return Event{ID: id, CreatedAt: time.Now(), Sender: from, Content: content}, nil
}
