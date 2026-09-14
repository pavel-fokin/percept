const assert = require("node:assert/strict");
const test = require("node:test");

const { INTERVAL_MS, TOKENS } = require("../lib/fake-generator");
const { createGeneration, delay, openEvents, readThrough, startServer } = require("./support");

test("the browser app and its script are served", { timeout: 3000 }, async (context) => {
  const app = await startServer();
  context.after(app.close);

  const page = await fetch(`${app.base}/`);
  const script = await fetch(`${app.base}/app.js`);

  assert.equal(page.status, 200);
  assert.match(await page.text(), /Browser chat/i);
  assert.equal(script.status, 200);
  assert.match(await script.text(), /\/api\/generations/);
});

test("one connected stream receives the deterministic generation", { timeout: 3000 }, async (context) => {
  const app = await startServer();
  context.after(app.close);
  const id = await createGeneration(app.base);

  const stream = await openEvents(app.base, id);
  assert.equal(stream.response.status, 200);
  const events = await readThrough(stream, "completed");

  assert.deepEqual(
    events.filter(({ event }) => event === "token").map(({ data }) => data.token),
    TOKENS,
  );
  assert.equal(events.at(-1).data.text, TOKENS.join(""));
});

test("disconnect stops and removes the generation", { timeout: 3000 }, async (context) => {
  const app = await startServer();
  context.after(app.close);
  const id = await createGeneration(app.base);
  const stream = await openEvents(app.base, id);

  assert.equal((await stream.next()).event, "token");
  stream.close();
  await delay(INTERVAL_MS * 2);

  const reconnect = await fetch(`${app.base}/api/generations/${id}/events`);
  assert.equal(reconnect.status, 404);
});
