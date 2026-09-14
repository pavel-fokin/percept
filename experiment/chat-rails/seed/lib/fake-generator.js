const TOKENS = ["Rails", " keep", " changing", " decisions", " visible", "."];
const INTERVAL_MS = 30;

function startGeneration(onToken, onComplete) {
  let index = 0;
  const timer = setInterval(() => {
    onToken(TOKENS[index], index);
    index += 1;
    if (index === TOKENS.length) {
      clearInterval(timer);
      onComplete(TOKENS.join(""));
    }
  }, INTERVAL_MS);

  return () => clearInterval(timer);
}

module.exports = { INTERVAL_MS, TOKENS, startGeneration };
