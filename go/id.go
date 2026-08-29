package main

import "github.com/google/uuid"

// EventID uniquely identifies an Event. Per ADR, entity IDs use UUIDv7 -
// time-ordered, so IDs sort by creation - wrapped in a type dedicated to
// that entity, so IDs from different entities can't be swapped by mistake.
type EventID uuid.UUID

func newEventID() (EventID, error) {
	id, err := uuid.NewV7()
	if err != nil {
		return EventID{}, err
	}
	return EventID(id), nil
}

func (id EventID) String() string {
	return uuid.UUID(id).String()
}
