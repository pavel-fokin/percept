package main

type sender int

const (
	senderUser sender = iota
	senderAssistant
)

type chatMessage struct {
	from sender
	text string
}

func stubAssistantReply(userText string) string {
	return "You said: " + userText
}
