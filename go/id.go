package main

import "github.com/google/uuid"

// ID is a phantom-typed UUID: T pins it to one entity type, so IDs from
// different entities can't be mixed up. Per ADR, entity IDs use UUIDv7 -
// time-ordered, so IDs sort by creation. uuid.UUID is embedded, not
// wrapped by a plain type definition, so ID[T] keeps its promoted
// methods (String, MarshalText, database/sql support) for free.
type ID[T any] struct{ uuid.UUID }

func NewID[T any]() (ID[T], error) {
	u, err := uuid.NewV7()
	if err != nil {
		return ID[T]{}, err
	}
	return ID[T]{u}, nil
}
