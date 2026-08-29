package providers

import (
	"context"
	"math/rand"
	"strings"
	"time"

	"github.com/pavel-fokin/percept/go/internal/percept"
)

// staticReply is what Stub streams back, regardless of input - static
// text for now, standing in for a real model's generated tokens.
const staticReply = "This is a simulated streaming reply. Words appear one at a time, just like a real language model would send them."

// Stub streams staticReply word by word: an initial random 0.5-1.5s
// delay (time to "first token"), then ~40-120ms between words - long
// enough, at both points, to make the streaming actually visible.
type Stub struct{}

func (Stub) Reply(_ context.Context, _ []percept.Message) (<-chan string, error) {
	chunks := make(chan string)
	go func() {
		defer close(chunks)
		time.Sleep(500*time.Millisecond + time.Duration(rand.Int63n(int64(time.Second))))
		for i, word := range strings.Fields(staticReply) {
			chunk := word
			if i > 0 {
				chunk = " " + word
			}
			chunks <- chunk
			time.Sleep(40*time.Millisecond + time.Duration(rand.Int63n(80*int64(time.Millisecond))))
		}
	}()
	return chunks, nil
}
