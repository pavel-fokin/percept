const http = require("node:http");

function createServer() {
  return http.createServer((_request, response) => {
    response.writeHead(501, { "content-type": "application/json" });
    response.end(JSON.stringify({ error: "chat generation is not implemented" }));
  });
}

if (require.main === module) {
  const port = Number(process.env.PORT || 3000);
  createServer().listen(port, () => console.log(`listening on http://localhost:${port}`));
}

module.exports = { createServer };
