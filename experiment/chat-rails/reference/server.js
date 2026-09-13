const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");
const { startGeneration } = require("./lib/fake-generator");

const stage = Number(process.env.CHAT_REFERENCE_STAGE || 3);

function sendEvent(response, event, data) {
  response.write(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`);
}

function createServer() {
  const generations = new Map();
  let nextId = 1;

  return http.createServer(async (request, response) => {
    const url = new URL(request.url, "http://localhost");
    if (request.method === "POST" && url.pathname === "/api/generations") {
      for await (const _chunk of request) {}
      const id = String(nextId++);
      generations.set(id, {
        clients: new Set(),
        events: [],
        started: false,
        finished: false,
        stop: null,
      });
      response.writeHead(201, { "content-type": "application/json" });
      response.end(JSON.stringify({ id }));
      return;
    }

    const match = url.pathname.match(/^\/api\/generations\/([^/]+)(?:\/events)?$/);
    const generation = match && generations.get(match[1]);
    if (request.method === "GET" && match && url.pathname.endsWith("/events")) {
      if (!generation) {
        response.writeHead(404).end();
        return;
      }
      response.writeHead(200, {
        "cache-control": "no-cache",
        connection: "keep-alive",
        "content-type": "text/event-stream",
      });
      generation.clients.add(response);
      for (const event of generation.events) sendEvent(response, event.event, event.data);
      if (generation.finished) {
        response.end();
        return;
      }
      if (!generation.started) {
        generation.started = true;
        generation.stop = startGeneration(
          (token, index) => {
            const event = { event: "token", data: { token, index } };
            generation.events.push(event);
            for (const client of generation.clients) sendEvent(client, event.event, event.data);
          },
          (text) => {
            const event = { event: "completed", data: { text } };
            generation.events.push(event);
            generation.finished = true;
            for (const client of generation.clients) {
              sendEvent(client, event.event, event.data);
              client.end();
            }
            generation.clients.clear();
          },
        );
      }
      request.on("close", () => {
        generation.clients.delete(response);
        if (stage === 1 && !generation.finished) {
          generation.stop();
          generations.delete(match[1]);
        }
      });
      return;
    }

    if (request.method === "DELETE" && match && !url.pathname.endsWith("/events")) {
      if (stage < 3 || !generation) {
        response.writeHead(404).end();
        return;
      }
      generation.stop?.();
      generation.finished = true;
      const event = { event: "cancelled", data: {} };
      generation.events.push(event);
      for (const client of generation.clients) {
        sendEvent(client, event.event, event.data);
        client.end();
      }
      generation.clients.clear();
      response.writeHead(202).end();
      return;
    }

    if (request.method === "GET" && (url.pathname === "/" || url.pathname === "/app.js")) {
      const file = url.pathname === "/" ? "index.html" : "app.js";
      response.writeHead(200, {
        "content-type": file.endsWith(".html") ? "text/html" : "text/javascript",
      });
      response.end(fs.readFileSync(path.join(__dirname, "public", file)));
      return;
    }

    response.writeHead(404).end();
  });
}

if (require.main === module) {
  const port = Number(process.env.PORT || 3000);
  createServer().listen(port, () => console.log(`listening on http://localhost:${port}`));
}

module.exports = { createServer };
