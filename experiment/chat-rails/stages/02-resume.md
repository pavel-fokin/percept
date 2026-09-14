The product requirement changed. A generation must survive when its
browser stream disconnects. A later GET for the same id must replay all
tokens produced so far, then stream new tokens through completion.

Update the backend and browser client for reconnect and resume. Keep the
existing HTTP and SSE contract. Run all tests and finish the implementation.
