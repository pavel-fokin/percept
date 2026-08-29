package providers

import (
	"context"
	"math/rand"
	"time"

	"github.com/pavel-fokin/percept/go/internal/percept"
)

// Stub echoes the last user message back, prefixed, after a random
// 0.5-1.5s delay - long enough to make the async reply fetch's gap
// actually observable.
type Stub struct{}

func (Stub) Reply(_ context.Context, messages []percept.Message) (string, error) {
	time.Sleep(500*time.Millisecond + time.Duration(rand.Int63n(int64(time.Second))))
	for i := len(messages) - 1; i >= 0; i-- {
		if messages[i].Role == percept.RoleUser {
			return "You said: " + messages[i].Content, nil
		}
	}
	return "", nil
}
