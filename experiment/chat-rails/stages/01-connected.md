Implement the first usable version of this browser chat. Keep the Node
standard library and the supplied deterministic fake generator.

The HTTP contract is:

- `POST /api/generations` accepts JSON with a `prompt` string and returns
  status 201 with `{ "id": "..." }`.
- `GET /api/generations/:id/events` is an SSE stream. It emits `token`
  events with `{ "token": "...", "index": n }`, then one `completed`
  event with `{ "text": "..." }`.
- A generation lives only while that stream is connected. Closing the
  stream stops and removes it. A later GET for the id returns 404.
- The server exports `createServer()` and serves the browser files.

Connect the page to this API. Run all tests and finish the implementation.
