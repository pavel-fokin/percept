const assert = require("node:assert/strict");
const test = require("node:test");

const { createGeneration, delay, openEvents, readThrough, startServer } = require("./support");

const generatorPath = require.resolve("../lib/fake-generator");
const generator = require(generatorPath);
const { INTERVAL_MS, TOKENS } = generator;
let generatedTokens = 0;
let generationStarts = 0;
require.cache[generatorPath].exports = {
  ...generator,
  startGeneration(onToken, onComplete) {
    generationStarts += 1;
    return generator.startGeneration((token, index) => {
      generatedTokens += 1;
      onToken(token, index);
    }, onComplete);
  },
};

test("a disconnected generation continues and replays on reconnect", { timeout: 3000 }, async (context) => {
  const app = await startServer();
  context.after(app.close);
  const id = await createGeneration(app.base);
  const first = await openEvents(app.base, id);

  assert.equal((await first.next()).event, "token");
  first.close();
  await delay(INTERVAL_MS * 1.5);
  assert.ok(generatedTokens > 1 && generatedTokens < TOKENS.length);

  const resumed = await openEvents(app.base, id);
  assert.equal(resumed.response.status, 200);
  const events = await readThrough(resumed, "completed");
  assert.deepEqual(
    events.filter(({ event }) => event === "token").map(({ data }) => data.token),
    TOKENS,
  );
  assert.equal(events.at(-1).data.text, TOKENS.join(""));
  assert.equal(generationStarts, 1);
});
