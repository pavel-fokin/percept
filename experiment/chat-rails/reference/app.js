const form = document.querySelector("#chat-form");
const cancel = document.querySelector("#cancel");
const reply = document.querySelector("#reply");
let generationId = location.hash.slice(1);
let connection;

async function connect(id) {
  connection?.abort();
  connection = new AbortController();
  cancel.disabled = false;
  const response = await fetch(`/api/generations/${id}/events`, {
    signal: connection.signal,
  });
  if (!response.ok) {
    reply.textContent = "Generation is unavailable.";
    return;
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffered = "";
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      buffered += decoder.decode(value, { stream: true }).replace(/\r\n/g, "\n");
      let boundary;
      while ((boundary = buffered.indexOf("\n\n")) !== -1) {
        const block = buffered.slice(0, boundary);
        buffered = buffered.slice(boundary + 2);
        const event = block.match(/^event: (.+)$/m)?.[1];
        const data = JSON.parse(block.match(/^data: (.+)$/m)?.[1] || "{}");
        if (event === "token") reply.textContent += data.token;
        if (event === "cancelled") reply.textContent += "\nCancelled";
        if (event === "completed" || event === "cancelled") cancel.disabled = true;
      }
    }
  } catch (error) {
    if (error.name !== "AbortError") setTimeout(() => connect(id), 100);
  }
}

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  reply.textContent = "";
  const response = await fetch("/api/generations", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ prompt: new FormData(form).get("prompt") }),
  });
  ({ id: generationId } = await response.json());
  location.hash = generationId;
  connect(generationId);
});

cancel.addEventListener("click", () =>
  fetch(`/api/generations/${generationId}`, { method: "DELETE" }),
);

if (generationId) connect(generationId);
