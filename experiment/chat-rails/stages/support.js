const assert = require("node:assert/strict");

async function startServer() {
  const { createServer } = require("../server");
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const { port } = server.address();
  return {
    base: `http://127.0.0.1:${port}`,
    close: () =>
      new Promise((resolve) => {
        server.closeAllConnections();
        server.close(resolve);
      }),
  };
}

async function createGeneration(base) {
  const response = await fetch(`${base}/api/generations`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ prompt: "Explain cognitive rails" }),
  });
  assert.equal(response.status, 201);
  const body = await response.json();
  assert.equal(typeof body.id, "string");
  assert.notEqual(body.id, "");
  return body.id;
}

async function openEvents(base, id) {
  const controller = new AbortController();
  const response = await fetch(`${base}/api/generations/${id}/events`, {
    signal: controller.signal,
  });
  if (!response.ok) {
    return { response, close: () => controller.abort() };
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffered = "";
  const pending = [];

  async function next() {
    while (pending.length === 0) {
      const { done, value } = await reader.read();
      if (done) return undefined;
      buffered += decoder.decode(value, { stream: true }).replace(/\r\n/g, "\n");
      let boundary;
      while ((boundary = buffered.indexOf("\n\n")) !== -1) {
        const block = buffered.slice(0, boundary);
        buffered = buffered.slice(boundary + 2);
        const event = block.match(/^event: (.+)$/m)?.[1];
        const data = block.match(/^data: (.+)$/m)?.[1];
        if (event && data) pending.push({ event, data: JSON.parse(data) });
      }
    }
    return pending.shift();
  }

  return {
    response,
    next,
    close() {
      controller.abort();
      reader.cancel().catch(() => {});
    },
  };
}

async function readThrough(stream, wanted) {
  const events = [];
  for (;;) {
    const event = await stream.next();
    assert.ok(event, `stream ended before ${wanted}`);
    events.push(event);
    if (event.event === wanted) return events;
  }
}

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

module.exports = { createGeneration, delay, openEvents, readThrough, startServer };
