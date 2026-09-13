const form = document.querySelector("#chat-form");
const cancel = document.querySelector("#cancel");
const reply = document.querySelector("#reply");

form.addEventListener("submit", (event) => {
  event.preventDefault();
  reply.textContent = "Generation is not implemented.";
});
