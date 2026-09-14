const assert = require("node:assert/strict");
const test = require("node:test");

const { createGeneration, openEvents, readThrough, startServer } = require("./support");

const generatorPath = require.resolve("../lib/fake-generator");
const generator = require(generatorPath);
let generationStarts = 0;
require.cache[generatorPath].exports = {
  ...generator,
  startGeneration(...arguments_) {
    generationStarts += 1;
    return generator.startGeneration(...arguments_);
  },
};

test("two tabs share one generation and observe its cancellation", { timeout: 3000 }, async (context) => {
  const app = await startServer();
  context.after(app.close);
  const id = await createGeneration(app.base);
  const first = await openEvents(app.base, id);
  const second = await openEvents(app.base, id);

  const [firstBeforeCancel, secondBeforeCancel] = await Promise.all([
    readThrough(first, "token"),
    readThrough(second, "token"),
  ]);
  assert.deepEqual(firstBeforeCancel, secondBeforeCancel);
  assert.equal(generationStarts, 1);

  const cancelled = await fetch(`${app.base}/api/generations/${id}`, { method: "DELETE" });
  assert.equal(cancelled.status, 202);
  const [firstAfterCancel, secondAfterCancel] = await Promise.all([
    readThrough(first, "cancelled"),
    readThrough(second, "cancelled"),
  ]);
  assert.deepEqual(firstAfterCancel, secondAfterCancel);
  assert.equal(firstAfterCancel.some(({ event }) => event === "completed"), false);
});

test("the browser client can cancel through the shared API", { timeout: 3000 }, async (context) => {
  const app = await startServer();
  context.after(app.close);

  const script = await fetch(`${app.base}/app.js`);
  assert.equal(script.status, 200);
  assert.match(await script.text(), /method:\s*["']DELETE["']/);
});
